//! J_nu(x), Y_nu(x) and the Airy functions Ai, Bi, Ai', Bi' for machine reals, in double-double (decision SF.3).
//!
//! - **Steed's method** (Barnett's `bessjy`): with nu = mu + n, the continued fraction CF1 gives J'_nu/J_nu, the
//!   downward recurrence carries it to mu (|mu| <= 1/2 for x < 2, mu about x - 1 otherwise); Temme's series
//!   (x < 2, the gamma factors from the Taylor coefficients of 1/Gamma(1 + z)) or the complex continued fraction
//!   CF2 (x >= 2) give Y_mu and Y_mu+1 or (J' + iY')/(J + iY); the Wronskian J_mu Y'_mu - J'_mu Y_mu = 2/(pi x)
//!   fixes the scale, and the forward recurrence (stable for Y) reaches Y_nu.
//! - **Large x** (x >= 25): Hankel's expansion J = sqrt(2/(pi x)) (P cos w - Q sin w), Y = sqrt(2/(pi x))
//!   (P sin w + Q cos w), w = x - (nu/2 + 1/4) pi, used when its terms fall below 1e-33 with at most 2^30 of
//!   cancellation; the phase x/pi mod 2 is computed in double-double below 2^28 and from a ball of pi (128 bits
//!   beyond the integer bits of x) above.
//! - **Tiny x** (x < 2^-60, where x^2/4 is below 2^-122 relative): J_nu = (x/2)^nu / Gamma(nu + 1); Y_mu, Y_mu+1 from
//!   the first terms of Temme's series, then the forward recurrence.
//! - **Negative orders:** J_-nu = cos(nu pi) J_nu - sin(nu pi) Y_nu, Y_-nu = sin(nu pi) J_nu + cos(nu pi) Y_nu
//!   ((-1)^n for integers; the reduction of nu pi is exact). This is a rotation, so the envelope
//!   sqrt(J^2 + Y^2) of order -nu is that of order nu.
//! - **Airy:** the Maclaurin series (Abramowitz and Stegun 10.4.2-3) for -3 <= x <= 2.5 (and for Bi, Bi' up to 12,
//!   where they do not cancel); Ai, Ai' for x > 2.5 through K_1/3, K_2/3 at zeta = (2/3) x^(3/2) by Steed's CF2
//!   (Ai = sqrt(x/3)/pi K_1/3, Ai' = -x/(pi sqrt 3) K_2/3); Bi, Bi' for x > 12 by the expansions
//!   e^zeta/(sqrt(pi) x^(1/4)) sum a_k(1/3) (-zeta)^-k and x^(1/4) e^zeta/sqrt(pi) sum a_k(2/3) (-zeta)^-k
//!   (their exponentially small corrections are below e^-2 zeta < 1e-24); x < -3 through J and Y of orders 1/3, 2/3
//!   at zeta (DLMF 9.6.6-9), with Hankel's P, Q when they converge, the phase zeta/pi taken from a ball above
//!   |x| = 2^19.
//!
//! All values are carried as double-doubles (scaled where they leave the double range) and rounded once. The error
//! is a few units of 2^-100 relative to the envelope sqrt(J^2 + Y^2) (sqrt(Ai^2 + Bi^2) for x < 0) before that
//! rounding, so the machine values are within about an ulp away from the zeros and within an ulp of the envelope
//! near them (the tests measure both).
//!
//! Domains: J_nu(x) for x >= 0, and x < 0 for integer nu; Y_nu(x) for x >= 0 (Y_nu(0) = -inf for nu >= 0, as
//! the C library); |nu| <= 10^6; NaN otherwise. x > 10^6 with an order so large that Hankel's expansion does not
//! converge (|nu| above about sqrt(x)) gives NaN.
use super::gamma::gamma_scaled;
use super::{float_dd, rgamma_coefficients};
use crate::dd::{exp, exp_scaled, log, sin_cos_pi, sin_cos_pi_any, Scaled, DD, LN2, PI};
use mc_arith::num::{Ball, Float};
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::OnceLock;

/// Convergence of the continued fractions (about 2^-103: the double-double rounding of a ratio is near 2^-106).
const EPS_CF: f64 = 1e-31;
/// Convergence of the series.
const EPS: f64 = 1e-33;
/// The largest number of continued-fraction steps.
const MAXIT: u32 = 10_000_000;
/// The largest order handled.
const MAX_ORDER: f64 = 1e6;
/// 2^-60: below, only the leading terms of the series count.
const TINY: f64 = 8.673_617_379_884_035e-19;
/// Hankel's expansion is tried from here on.
const HANKEL_FROM: f64 = 25.0;
/// Below this, x/pi in double-double has an absolute error below 2^-75.
const PHASE_DD: f64 = 268_435_456.0;

/// The constants of this module, computed once.
struct Consts {
    inv_pi: DD,
    sqrt3: DD,
    inv_sqrt_pi: DD,
    third: DD,
    two_thirds: DD,
    /// Ai(0) = 3^(-2/3)/Gamma(2/3)
    c1: DD,
    /// -Ai'(0) = 3^(-1/3)/Gamma(1/3)
    c2: DD,
}

fn consts() -> &'static Consts {
    static C: OnceLock<Consts> = OnceLock::new();
    C.get_or_init(|| {
        let _scope = mc_arith::num::StopScope::enter(None);
        let (c1, c2) = crate::ball::bessel_j::airy_constants(256).expect("the Airy constants");
        Consts {
            inv_pi: DD::ONE.div(PI),
            sqrt3: DD::new(3.0).sqrt(),
            inv_sqrt_pi: PI.sqrt().recip(),
            third: DD::ONE.div_f(3.0),
            two_thirds: DD::new(2.0).div_f(3.0),
            c1: float_dd(&c1.mid),
            c2: float_dd(&c2.mid),
        }
    })
}

/// A scaled double-double as a double-double (for values in the double range).
fn sd(s: Scaled) -> DD {
    s.m.ldexp(s.e.clamp(-2000, 2000) as i32)
}

/// 1/x as a scaled double-double, for any x > 0 (also subnormal).
fn recip_scaled(x: DD) -> Scaled {
    if x.hi < 1e-290 {
        let mut s = Scaled::new(x.mul_f(crate::dd::pow2(600)).recip());
        s.e += 600;
        return s;
    }
    Scaled::new(x.recip())
}

/// ln(x/2) for any x > 0 (also subnormal).
fn ln_half(x: DD) -> DD {
    if x.hi < 1e-290 {
        return log(x.mul_f(crate::dd::pow2(600))).sub(LN2.mul_f(601.0));
    }
    log(x).sub(LN2)
}

/// A ball divided by pi, reduced modulo 2, as a double-double (the phase of huge arguments).
fn ball_phase(v: &Ball) -> DD {
    let _scope = mc_arith::num::StopScope::enter(None);
    let wp = v.mid.top().max(0) as u64 + 128;
    let Some(q) = crate::ball::pi(wp).and_then(|pi| v.div(&pi, wp)) else {
        return DD::new(f64::NAN);
    };
    let m = &q.mid;
    if m.exp >= 1 {
        return DD::ZERO; // an even integer
    }
    let s = (-m.exp) as u64;
    let k = m.man.shr(s + 1);
    let r = Float::new(false, m.man.clone(), m.exp).sub_exact(&Float::new(false, k, 1));
    let d = float_dd(&r);
    if m.neg {
        d.neg()
    } else {
        d
    }
}

/// The exact ball of a double-double.
fn dd_ball(x: DD) -> Ball {
    let f = crate::ball::float_of(x.hi).add_exact(&crate::ball::float_of(x.lo));
    Ball::exact(f)
}

/// x/pi modulo 2 for x >= 0, so that sin(pi t), cos(pi t) are sin x, cos x.
fn phase(x: DD) -> DD {
    if x.hi < PHASE_DD {
        let t = x.mul(consts().inv_pi);
        let n = (t.hi * 0.5).round() * 2.0;
        return DD::sum(t.hi - n, t.lo);
    }
    ball_phase(&dd_ball(x))
}

/// Hankel's P and Q of order nu at 1/x = `xinv`, if the terms fall below 1e-33 with little cancellation.
fn hankel_pq(nu: DD, xinv: DD) -> Option<(DD, DD)> {
    let mu4 = nu.sqr().mul_f(4.0);
    let r = xinv.mul_f(0.125);
    let (mut p, mut q) = (DD::ZERO, DD::ZERO);
    let mut t = DD::ONE;
    let mut prev = f64::INFINITY;
    let mut big = 1.0f64;
    let turn = 2.0 * nu.hi.abs() + 4.0;
    for k in 0..600u32 {
        let a = t.hi.abs();
        if a < EPS {
            return (big < 1e9).then_some((p, q));
        }
        if k as f64 > turn && a > prev {
            return None;
        }
        big = big.max(a);
        prev = a;
        match k % 4 {
            0 => p = p.add(t),
            1 => q = q.add(t),
            2 => p = p.sub(t),
            _ => q = q.sub(t),
        }
        let j = (2 * k + 1) as f64;
        t = t.mul(mu4.add_f(-j * j)).mul(r).div_f((k + 1) as f64);
    }
    None
}

/// (J_nu(x), Y_nu(x)) by Hankel's expansion, if it converges.
fn hankel_jy(nu: DD, x: DD) -> Option<(DD, DD)> {
    let (p, q) = hankel_pq(nu, x.recip())?;
    let th = phase(x).sub(nu.mul_f(0.5)).add_f(-0.25);
    let (s, c) = sin_cos_pi_any(th);
    let a = consts().inv_pi.mul_f(2.0).div(x).sqrt();
    Some((a.mul(p.mul(c).sub(q.mul(s))), a.mul(p.mul(s).add(q.mul(c)))))
}

/// The leading terms of Temme's series for |mu| <= 1/2 and d = -ln(x/2): (f_0, p_0, q_0, r) with
/// Y_mu = -(sum of c_k (f_k + r q_k)) and Y_mu+1 = -(2/x) (sum of c_k (p_k - k f_k)), c_k = (-x^2/4)^k/k!.
fn temme_start(mu: DD, d: DD) -> (DD, DD, DD, DD) {
    let g = rgamma_coefficients();
    let c = consts();
    let fact = if mu.hi == 0.0 { DD::ONE } else { PI.mul(mu).div(sin_cos_pi(mu).0) };
    let e = mu.mul(d);
    let (ep, em) = (exp(e), exp(e.neg()));
    let fact2 = if e.hi.abs() < 1e-3 {
        // sinh(e)/e = 1 + e^2/6 + e^4/120 + ...
        let e2 = e.sqr();
        let mut s = DD::ONE;
        let mut t = DD::ONE;
        for k in 1..=8 {
            t = t.mul(e2).div_f(((2 * k) * (2 * k + 1)) as f64);
            s = s.add(t);
        }
        s
    } else {
        ep.sub(em).div(e.mul_f(2.0))
    };
    let cosh = ep.add(em).mul_f(0.5);
    // gam1 = (1/Gamma(1-mu) - 1/Gamma(1+mu))/(2 mu) = -sum_{k odd} g_k mu^(k-1); gam2 = sum_{k even} g_k mu^k
    let m2 = mu.sqr();
    let (mut gam1, mut gam2) = (DD::ZERO, DD::ZERO);
    for k in (0..g.len()).rev() {
        if k % 2 == 1 {
            gam1 = gam1.mul(m2).add(g[k]);
        } else {
            gam2 = gam2.mul(m2).add(g[k]);
        }
    }
    let gam1 = gam1.neg();
    let (mut gampl, mut gammi) = (DD::ZERO, DD::ZERO);
    for k in g.iter().rev() {
        gampl = gampl.mul(mu).add(*k);
        gammi = gammi.mul(mu.neg()).add(*k);
    }
    let ff = c.inv_pi.mul_f(2.0).mul(fact).mul(gam1.mul(cosh).add(gam2.mul(fact2).mul(d)));
    let p = ep.div(gampl.mul(PI));
    let q = em.div(gammi.mul(PI));
    // r = pi (pi mu/2) (sin(pi mu/2)/(pi mu/2))^2 = 2 sin^2(pi mu/2) / mu
    let r = if mu.hi == 0.0 { DD::ZERO } else { sin_cos_pi(mu.mul_f(0.5)).0.sqr().mul_f(2.0).div(mu) };
    (ff, p, q, r)
}

/// The forward recurrence Y_{k+1} = (2k/x) Y_k - Y_{k-1} from (Y_mu, Y_mu+1) to Y_mu+n.
fn y_up(mu: DD, n: u64, xi2: Scaled, (mut y0, mut y1): (Scaled, Scaled)) -> Scaled {
    if n == 0 {
        return y0;
    }
    for i in 1..n {
        let t = y1.mul(xi2).mul_dd(mu.add_f(i as f64)).add(y0.neg());
        y0 = y1;
        y1 = t;
    }
    y1
}

/// (J_nu, Y_nu) for x < 2^-60 (nu >= 0).
fn jy_tiny(nu: DD, x: DD) -> (Scaled, Scaled) {
    let l = ln_half(x);
    let j = exp_scaled(nu.mul(l)).div(gamma_scaled(nu.add_f(1.0)));
    let n = nu.hi.round();
    let mu = nu.add_f(-n);
    let (ff, p, q, r) = temme_start(mu, l.neg());
    let xi2 = recip_scaled(x).mul_dd(DD::new(2.0));
    let y0 = Scaled::new(ff.add(r.mul(q)).neg());
    let y1 = Scaled::new(p.neg()).mul(xi2);
    (j, y_up(mu, n as u64, xi2, (y0, y1)))
}

/// (J_nu, Y_nu) by Steed's method for 2^-60 <= x (nu >= 0).
fn jy_steed(nu: DD, x: DD) -> (Scaled, Scaled) {
    let c = consts();
    let nl = if x.hi < 2.0 { nu.hi.round() } else { (nu.hi - x.hi + 1.5).floor().max(0.0) };
    let mu = nu.add_f(-nl);
    let xi = x.recip();
    let xi2 = xi.mul_f(2.0);
    let w = xi2.mul(c.inv_pi);
    // CF1 for h = J'_nu / J_nu (modified Lentz), counting the sign changes of the denominators
    let tiny = 1e-300;
    let mut h = nu.mul(xi);
    if h.hi.abs() < tiny {
        h = DD::new(tiny);
    }
    let mut b = xi2.mul(nu);
    let mut d = DD::ZERO;
    let mut cc = h;
    let mut isign = 1.0;
    for _ in 0..MAXIT {
        b = b.add(xi2);
        d = b.sub(d);
        if d.hi.abs() < tiny {
            d = DD::new(tiny);
        }
        cc = b.sub(cc.recip());
        if cc.hi.abs() < tiny {
            cc = DD::new(tiny);
        }
        d = d.recip();
        let del = cc.mul(d);
        h = del.mul(h);
        if d.hi < 0.0 {
            isign = -isign;
        }
        if del.add_f(-1.0).hi.abs() < EPS_CF {
            break;
        }
    }
    // the downward recurrence from nu to mu, from J_nu = isign (scaled by 2^-ex when it grows)
    let rjl1 = DD::new(isign);
    let mut rjl = rjl1;
    let mut rjpl = h.mul(rjl1);
    let mut ex: i64 = 0;
    let mut fact = nu.mul(xi);
    for _ in 0..nl as u64 {
        let t = fact.mul(rjl).add(rjpl);
        fact = fact.sub(xi);
        rjpl = fact.mul(t).sub(rjl);
        rjl = t;
        if rjl.hi.abs().max(rjpl.hi.abs()) > 1e240 {
            rjl = rjl.ldexp(-700);
            rjpl = rjpl.ldexp(-700);
            ex += 700;
        }
    }
    if rjl.hi == 0.0 {
        rjl = DD::new(EPS);
    }
    let f = rjpl.div(rjl);
    let (rjmu, rymu, ry1) = if x.hi < 2.0 {
        let (mut ff, mut p, mut q, r) = temme_start(mu, log(x.mul_f(0.5)).neg());
        let x2 = x.mul_f(0.5);
        let dq = x2.sqr().neg();
        let mu2 = mu.sqr();
        let mut ck = DD::ONE;
        let mut sum = ff.add(r.mul(q));
        let mut sum1 = p;
        for i in 1..MAXIT {
            let fi = i as f64;
            ff = ff.mul_f(fi).add(p).add(q).div(DD::new(fi * fi).sub(mu2));
            ck = ck.mul(dq).div_f(fi);
            p = p.div(DD::new(fi).sub(mu));
            q = q.div(DD::new(fi).add(mu));
            let del = ck.mul(ff.add(r.mul(q)));
            sum = sum.add(del);
            let del1 = ck.mul(p).sub(del.mul_f(fi));
            sum1 = sum1.add(del1);
            if del.hi.abs() < (1.0 + sum.hi.abs()) * EPS {
                break;
            }
        }
        let rymu = sum.neg();
        let ry1 = sum1.mul(xi2).neg();
        let rymup = mu.mul(xi).mul(rymu).sub(ry1);
        let rjmu = w.div(rymup.sub(f.mul(rymu)));
        (rjmu, rymu, ry1)
    } else {
        // CF2: p + iq = (J'_mu + i Y'_mu) / (J_mu + i Y_mu) (modified Lentz in complex arithmetic)
        let mut a = DD::new(0.25).sub(mu.sqr());
        let mut p = xi.mul_f(-0.5);
        let mut q = DD::ONE;
        let br = x.mul_f(2.0);
        let mut bi = DD::new(2.0);
        let fct = a.mul(xi).div(p.sqr().add(q.sqr()));
        let mut cr = br.add(q.mul(fct));
        let mut ci = bi.add(p.mul(fct));
        let den = br.sqr().add(bi.sqr());
        let mut dr = br.div(den);
        let mut di = bi.neg().div(den);
        let mut dlr = cr.mul(dr).sub(ci.mul(di));
        let mut dli = cr.mul(di).add(ci.mul(dr));
        let temp = p.mul(dlr).sub(q.mul(dli));
        q = p.mul(dli).add(q.mul(dlr));
        p = temp;
        for i in 2..MAXIT {
            a = a.add_f(2.0 * (i - 1) as f64);
            bi = bi.add_f(2.0);
            dr = a.mul(dr).add(br);
            di = a.mul(di).add(bi);
            if dr.hi.abs() + di.hi.abs() < tiny {
                dr = DD::new(tiny);
            }
            let fct = a.div(cr.sqr().add(ci.sqr()));
            cr = br.add(cr.mul(fct));
            ci = bi.sub(ci.mul(fct));
            if cr.hi.abs() + ci.hi.abs() < tiny {
                cr = DD::new(tiny);
            }
            let den = dr.sqr().add(di.sqr());
            dr = dr.div(den);
            di = di.neg().div(den);
            dlr = cr.mul(dr).sub(ci.mul(di));
            dli = cr.mul(di).add(ci.mul(dr));
            let temp = p.mul(dlr).sub(q.mul(dli));
            q = p.mul(dli).add(q.mul(dlr));
            p = temp;
            if dlr.add_f(-1.0).hi.abs() + dli.hi.abs() < EPS_CF {
                break;
            }
        }
        let pf = p.sub(f);
        let gam = pf.div(q);
        let mut rjmu = w.div(pf.mul(gam).add(q)).sqrt();
        if (rjl.hi < 0.0) != (rjmu.hi < 0.0) {
            rjmu = rjmu.neg();
        }
        let rymu = rjmu.mul(gam);
        // Y'_mu = Y_mu p + J_mu q (no division by gam, which may vanish)
        let rymup = rymu.mul(p).add(rjmu.mul(q));
        let ry1 = mu.mul(xi).mul(rymu).sub(rymup);
        (rjmu, rymu, ry1)
    };
    let mut jn = Scaled::new(rjmu.div(rjl).mul(rjl1));
    jn.e -= ex;
    let yn = y_up(mu, nl as u64, Scaled::new(xi2), (Scaled::new(rymu), Scaled::new(ry1)));
    (jn, yn)
}

/// (J_nu(x), Y_nu(x)) for nu >= 0 and finite x > 0, scaled; `None` where no method applies.
fn jy(nu: DD, x: DD) -> Option<(Scaled, Scaled)> {
    if x.hi >= HANKEL_FROM {
        if let Some((j, y)) = hankel_jy(nu, x) {
            return Some((Scaled::new(j), Scaled::new(y)));
        }
        if x.hi > 1e6 {
            return None;
        }
    }
    if x.hi < TINY {
        return Some(jy_tiny(nu, x));
    }
    Some(jy_steed(nu, x))
}

/// (J_nu(x), Y_nu(x)) for any order and x > 0, rounded.
fn jy_any(nu: f64, x: f64) -> Option<(f64, f64)> {
    let a = nu.abs();
    let (j, y) = jy(DD::new(a), DD::new(x))?;
    if nu >= 0.0 {
        return Some((j.to_f64(), y.to_f64()));
    }
    let (s, c) = sin_cos_pi_any(DD::new(a));
    // cos(a pi) vanishes exactly at half-integers (the series leaves about 1e-33 there)
    let c = if (2.0 * a) % 2.0 == 1.0 { DD::ZERO } else { c };
    let js = j.mul_dd(c).add(y.mul_dd(s).neg());
    let ys = j.mul_dd(s).add(y.mul_dd(c));
    Some((js.to_f64(), ys.to_f64()))
}

/// Is nu an odd integer?
fn odd(nu: f64) -> bool {
    nu.abs() % 2.0 == 1.0
}

/// J_nu(x) for machine reals: x >= 0, or any x for integer nu (J_n(-x) = (-1)^n J_n(x)); NaN otherwise.
pub fn bessel_j(nu: f64, x: f64) -> f64 {
    if nu.is_nan() || x.is_nan() || nu.is_infinite() || nu.abs() > MAX_ORDER {
        return f64::NAN;
    }
    let int_order = nu == nu.floor();
    if x < 0.0 {
        if !int_order {
            return f64::NAN;
        }
        let v = bessel_j(nu, -x);
        return if odd(nu) { -v } else { v };
    }
    if x == 0.0 {
        return if nu == 0.0 {
            1.0
        } else if nu > 0.0 || int_order {
            0.0
        } else {
            // J_nu(x) ~ (x/2)^nu / Gamma(nu + 1)
            f64::INFINITY * super::gamma::gamma(nu + 1.0).signum()
        };
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    if int_order && nu < 0.0 {
        let v = bessel_j(-nu, x);
        return if odd(nu) { -v } else { v };
    }
    jy_any(nu, x).map_or(f64::NAN, |v| v.0)
}

/// Y_nu(x) for machine reals: x >= 0 (Y_nu(0) = -inf for nu >= 0); NaN for x < 0.
pub fn bessel_y(nu: f64, x: f64) -> f64 {
    if nu.is_nan() || x.is_nan() || nu.is_infinite() || nu.abs() > MAX_ORDER || x < 0.0 {
        return f64::NAN;
    }
    let int_order = nu == nu.floor();
    if int_order && nu < 0.0 {
        let v = bessel_y(-nu, x);
        return if odd(nu) { -v } else { v };
    }
    if x == 0.0 {
        if nu >= 0.0 {
            return f64::NEG_INFINITY;
        }
        // Y_-a = sin(a pi) J_a + cos(a pi) Y_a with Y_a -> -inf, J_a -> 0
        let (_, c) = sin_cos_pi_any(DD::new(-nu));
        return if c.hi == 0.0 { 0.0 } else { f64::NEG_INFINITY * c.hi.signum() };
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    jy_any(nu, x).map_or(f64::NAN, |v| v.1)
}

// ---------------------------------------------------------------- Airy

/// Which Airy function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Airy {
    Ai,
    Bi,
    AiP,
    BiP,
}

/// The Maclaurin series: (f, g) or (f', g') in double-double.
fn maclaurin(x: f64, deriv: bool) -> (DD, DD) {
    let xd = DD::new(x);
    let x2 = xd.sqr();
    let x3 = x2.mul_f(x);
    let ax3 = x3.hi.abs();
    // f = 1 + sum_{k>=1} u_k x, f' = sum 3k u_k with u_k = f_k x^(3k-1);
    // g = sum v_k x, g' = sum (3k+1) v_k with v_k = g_k x^3k
    let mut u = x2.div_f(6.0);
    let mut v = DD::ONE;
    let (mut f, mut g) = if deriv { (u.mul_f(3.0), DD::ONE) } else { (DD::ONE.add(u.mul_f(x)), xd) };
    let scale = 1.0 + x.abs();
    for k in 1..2000u32 {
        let k3 = 3.0 * k as f64;
        // v_k
        v = v.mul(x3).div_f(k3 * (k3 + 1.0));
        if deriv {
            g = g.add(v.mul_f(k3 + 1.0));
        } else {
            g = g.add(v.mul_f(x));
        }
        // u_{k+1}
        u = u.mul(x3).div_f((k3 + 2.0) * (k3 + 3.0));
        if deriv {
            f = f.add(u.mul_f(k3 + 3.0));
        } else {
            f = f.add(u.mul_f(x));
        }
        let size = (u.hi.abs() + v.hi.abs()) * scale * (k3 + 4.0);
        if ax3 < 0.5 * k3 * k3 && size < EPS * f.hi.abs().max(g.hi.abs()).max(1.0) {
            break;
        }
    }
    (f, g)
}

/// Steed's CF2 for K_mu(x), K_mu+1(x), |mu| <= 1/2, x > 1 (as in `machine::bessel`, with a double-double order).
fn steed_k(m: DD, x: DD) -> (Scaled, Scaled) {
    let mut b = x.add_f(1.0).mul_f(2.0);
    let mut d = b.recip();
    let mut delh = d;
    let mut h = d;
    let (mut q1, mut q2) = (DD::ZERO, DD::ONE);
    let a1 = DD::new(0.25).sub(m.sqr());
    let mut q = a1;
    let mut c = a1;
    let mut a = a1.neg();
    let mut s = DD::ONE.add(q.mul(delh));
    for i in 2..MAXIT {
        let fi = i as f64;
        a = a.add_f(-2.0 * (fi - 1.0));
        c = a.neg().mul(c).div_f(fi);
        let qnew = q1.sub(b.mul(q2)).div(a);
        q1 = q2;
        q2 = qnew;
        q = q.add(c.mul(qnew));
        if q2.hi.abs() < 1e-200 {
            // q_k decay while c_k grow; only the products c_k q_k matter
            q1 = q1.mul_f(1e200);
            q2 = q2.mul_f(1e200);
            c = c.mul_f(1e-200);
        }
        b = b.add_f(2.0);
        d = b.add(a.mul(d)).recip();
        delh = b.mul(d).add_f(-1.0).mul(delh);
        h = h.add(delh);
        let dels = q.mul(delh);
        s = s.add(dels);
        if dels.hi.abs() < s.hi.abs() * EPS {
            break;
        }
    }
    let h = a1.mul(h);
    let pre = PI.div(x.mul_f(2.0)).sqrt();
    let k0 = exp_scaled(x.neg()).mul_dd(pre.div(s));
    let k1 = k0.mul_dd(m.add(x).add_f(0.5).sub(h).div(x));
    (k0, k1)
}

/// zeta = (2/3) x^(3/2) for x > 0.
fn zeta_of(x: f64) -> DD {
    let xd = DD::new(x);
    xd.mul(xd.sqrt()).mul(consts().two_thirds)
}

/// Bi or Bi' for x > 12 by their asymptotic expansions.
fn bi_asymptotic(kind: Airy, x: f64) -> f64 {
    let c = consts();
    let zeta = zeta_of(x);
    let r = zeta.recip().neg();
    let nu = if kind == Airy::Bi { c.third } else { c.two_thirds };
    let mu4 = nu.sqr().mul_f(4.0);
    let mut t = DD::ONE;
    let mut s = DD::ZERO;
    let mut prev = f64::INFINITY;
    for k in 0..200u32 {
        let a = t.hi.abs();
        if a < EPS || a > prev {
            break;
        }
        prev = a;
        s = s.add(t);
        let j = (2 * k + 1) as f64;
        t = t.mul(mu4.add_f(-j * j)).mul(r).div_f(8.0 * (k + 1) as f64);
    }
    let x4 = DD::new(x).sqrt().sqrt();
    let pre = if kind == Airy::Bi { c.inv_sqrt_pi.div(x4) } else { c.inv_sqrt_pi.mul(x4) };
    exp_scaled(zeta).mul_dd(pre.mul(s)).to_f64()
}

/// Ai or Ai' for x > 2.5 through K_1/3 and K_2/3.
fn ai_positive(kind: Airy, x: f64) -> f64 {
    if x > 120.0 {
        return if kind == Airy::Ai { 0.0 } else { -0.0 };
    }
    let c = consts();
    let zeta = zeta_of(x);
    // mu = -1/3: K_-1/3 = K_1/3 and K_2/3
    let (k13, k23) = steed_k(c.third.neg(), zeta);
    let xd = DD::new(x);
    if kind == Airy::Ai {
        k13.mul_dd(xd.div_f(3.0).sqrt().mul(c.inv_pi)).to_f64()
    } else {
        k23.mul_dd(xd.mul(c.inv_pi).div(c.sqrt3).neg()).to_f64()
    }
}

/// The phase zeta/pi - 1/4 for zeta = (2/3) z^(3/2), z > 0.
fn airy_phase(z: f64, zeta: DD) -> DD {
    if zeta.hi < PHASE_DD {
        return phase(zeta).add_f(-0.25);
    }
    // zeta exactly from z, in ball arithmetic
    let _scope = mc_arith::num::StopScope::enter(None);
    let zb = Ball::exact(crate::ball::float_of(z));
    let wp = (1.5 * zb.mid.top().max(0) as f64) as u64 + 160;
    let Some(zt) = zb.sqrt(wp) else { return DD::new(f64::NAN) };
    let zt = zt.mul(&zb, wp).mul(&Ball::from_ratio(2, 3, wp), wp);
    ball_phase(&zt).add_f(-0.25)
}

/// The Airy functions for x < -3.
fn airy_negative(kind: Airy, x: f64) -> f64 {
    let c = consts();
    let z = -x;
    let deriv = matches!(kind, Airy::AiP | Airy::BiP);
    let nu = if deriv { c.two_thirds } else { c.third };
    let zd = DD::new(z);
    let sz = zd.sqrt();
    // 1/zeta = (3/2) z^(-3/2), without overflow
    let zinv = DD::new(1.5).div(zd).div(sz);
    if let Some((p, q)) = hankel_pq(nu, zinv) {
        let zeta = if z < 1e200 { zd.mul(sz).mul(c.two_thirds) } else { DD::new(f64::INFINITY) };
        let (s, co) = sin_cos_pi_any(airy_phase(z, zeta));
        let z4 = sz.sqrt();
        let pcqs = p.mul(co).sub(q.mul(s));
        let psqc = p.mul(s).add(q.mul(co));
        return match kind {
            Airy::Ai => pcqs.mul(c.inv_sqrt_pi).div(z4),
            Airy::Bi => psqc.mul(c.inv_sqrt_pi).div(z4).neg(),
            Airy::AiP => psqc.mul(c.inv_sqrt_pi).mul(z4),
            Airy::BiP => pcqs.mul(c.inv_sqrt_pi).mul(z4),
        }
        .to_f64();
    }
    let zeta = zd.mul(sz).mul(c.two_thirds);
    let Some((j, y)) = jy(nu, zeta) else { return f64::NAN };
    let (j, y) = (sd(j), sd(y));
    let r3 = c.sqrt3;
    match kind {
        Airy::Ai => sz.mul_f(0.5).mul(j.sub(y.div(r3))),
        Airy::Bi => sz.mul_f(-0.5).mul(j.div(r3).add(y)),
        Airy::AiP => zd.mul_f(0.5).mul(j.add(y.div(r3))),
        Airy::BiP => zd.mul_f(0.5).mul(j.div(r3).sub(y)),
    }
    .to_f64()
}

fn airy(kind: Airy, x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == f64::INFINITY {
        return match kind {
            Airy::Ai => 0.0,
            Airy::AiP => -0.0,
            _ => f64::INFINITY,
        };
    }
    if x == f64::NEG_INFINITY {
        return match kind {
            Airy::Ai | Airy::Bi => 0.0,
            _ => f64::NAN,
        };
    }
    let bi = matches!(kind, Airy::Bi | Airy::BiP);
    if (-3.0..=2.5).contains(&x) || (bi && (0.0..=12.0).contains(&x)) {
        let c = consts();
        let deriv = matches!(kind, Airy::AiP | Airy::BiP);
        let (f, g) = maclaurin(x, deriv);
        let (cf, cg) = (c.c1.mul(f), c.c2.mul(g));
        return if bi { c.sqrt3.mul(cf.add(cg)) } else { cf.sub(cg) }.to_f64();
    }
    if x > 0.0 {
        if bi {
            if x > 200.0 {
                return f64::INFINITY;
            }
            return bi_asymptotic(kind, x);
        }
        return ai_positive(kind, x);
    }
    airy_negative(kind, x)
}

/// Ai(x) for machine reals.
pub fn airy_ai(x: f64) -> f64 {
    airy(Airy::Ai, x)
}

/// Bi(x) for machine reals.
pub fn airy_bi(x: f64) -> f64 {
    airy(Airy::Bi, x)
}

/// Ai'(x) for machine reals.
pub fn airy_ai_prime(x: f64) -> f64 {
    airy(Airy::AiP, x)
}

/// Bi'(x) for machine reals.
pub fn airy_bi_prime(x: f64) -> f64 {
    airy(Airy::BiP, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulps(got: f64, want: f64) -> f64 {
        ((got - want) / want).abs() / f64::EPSILON
    }

    #[test]
    fn known_values() {
        assert!(ulps(bessel_j(0.0, 1.0), 0.765_197_686_557_966_6) <= 2.0);
        assert!(ulps(bessel_y(0.0, 1.0), 0.088_256_964_215_676_96) <= 2.0);
        assert!(ulps(bessel_j(1.0, 10.0), 0.043_472_746_168_861_44) <= 2.0);
        assert!(ulps(airy_ai(0.0), 0.355_028_053_887_817_2) <= 2.0);
        assert!(ulps(airy_bi(0.0), 0.614_926_627_446_000_7) <= 2.0);
        assert!(ulps(airy_ai(1.0), 0.135_292_416_312_881_4) <= 2.0);
        // Ai(-1), Bi(-1), Ai'(-1), Bi'(-1)
        assert!(ulps(airy_ai(-1.0), 0.535_560_883_292_352_1) <= 2.0);
        assert!(ulps(airy_bi(-1.0), 0.103_997_389_496_944_6) <= 2.0);
        assert!(ulps(airy_ai_prime(-1.0), -0.010_160_567_116_645_21) <= 4.0);
        assert!(ulps(airy_bi_prime(-1.0), 0.592_375_626_422_792_3) <= 2.0);
        // J_1/2(x) = sqrt(2/(pi x)) sin x, J_-1/2(x) = sqrt(2/(pi x)) cos x
        let w = |x: f64| (2.0 / (std::f64::consts::PI * x)).sqrt();
        assert!(ulps(bessel_j(0.5, 3.0), w(3.0) * 3f64.sin()) <= 4.0);
        assert!(ulps(bessel_j(-0.5, 0.7), w(0.7) * 0.7f64.cos()) <= 4.0);
        assert!(ulps(bessel_y(0.5, 100.0), -w(100.0) * 100f64.cos()) <= 4.0);
        assert!(ulps(bessel_j(0.5, 1e-10), w(1e-10) * 1e-10f64.sin()) <= 4.0);
        // J_0 at the double nearest its first zero 2.40482555769577276862...: -6.1087652597367303970...e-17
        assert!(ulps(bessel_j(0.0, 2.404_825_557_695_773), -6.108_765_259_736_73e-17) <= 2.0);
        // negative x for integer orders, x = 0, domains
        assert_eq!(bessel_j(3.0, -2.0), -bessel_j(3.0, 2.0));
        assert_eq!(bessel_j(0.0, 0.0), 1.0);
        assert_eq!(bessel_j(2.0, 0.0), 0.0);
        assert!(bessel_j(0.5, -1.0).is_nan());
        assert_eq!(bessel_y(0.0, 0.0), f64::NEG_INFINITY);
        assert!(bessel_y(1.0, -1.0).is_nan());
        assert_eq!(airy_ai(200.0), 0.0);
        assert_eq!(airy_bi(200.0), f64::INFINITY);
    }

    #[test]
    fn identities() {
        // Wronskians J_nu+1 Y_nu - J_nu Y_nu+1 = 2/(pi x), Ai Bi' - Ai' Bi = 1/pi
        for nu in [0.0, 0.3, 1.0, 2.5, 7.0, 20.0] {
            for x in [0.01, 0.9, 1.7, 2.3, 9.0, 33.0, 150.0] {
                let w = bessel_j(nu + 1.0, x) * bessel_y(nu, x) - bessel_j(nu, x) * bessel_y(nu + 1.0, x);
                let want = 2.0 / (std::f64::consts::PI * x);
                let scale = (bessel_j(nu, x) * bessel_y(nu + 1.0, x)).abs().max(want);
                assert!((w - want).abs() <= 1e-14 * scale, "{nu} {x}: {w} {want}");
            }
        }
        for x in [-1000.0, -40.0, -10.0, -4.0, -2.0, 0.0, 1.0, 3.0, 8.0, 15.0, 40.0] {
            let w = airy_ai(x) * airy_bi_prime(x) - airy_ai_prime(x) * airy_bi(x);
            let scale = (airy_ai_prime(x) * airy_bi(x)).abs().max(1.0 / std::f64::consts::PI);
            assert!((w - 1.0 / std::f64::consts::PI).abs() <= 1e-14 * scale, "{x}: {w}");
        }
    }

    #[test]
    fn huge_arguments() {
        // J_0(1e10) = sqrt(2/(pi x)) cos(x - pi/4) to leading order (P = 1 - 9/(128 x^2), Q = -1/(8x))
        let x = 1e10f64;
        let v = bessel_j(0.0, x);
        assert!(v.abs() < 1e-5 && v.is_finite());
        let y = bessel_y(0.0, x);
        let env = (2.0 / (std::f64::consts::PI * x)).sqrt();
        assert!((v * v + y * y - env * env).abs() < 1e-12 * env * env);
        assert!(airy_ai(-1e12).is_finite() && airy_bi(-1e30).is_finite());
    }
}
