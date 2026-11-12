#![allow(clippy::neg_cmp_op_on_partial_ord)]
//! Special functions added in round 28, on the working form of [`crate::cover`]: the
//! arguments as balls at the target precision plus the bits a series is expected to cancel, series summed in the
//! canonical arithmetic, the value rounded back to the arguments' kind.
//!
//! - **Scorer functions** `scorer_hi`, `scorer_gi` and their derivatives (DLMF 9.12): the Maclaurin series
//!   Hi(z) = 3^(-2/3)/pi sum_k Gamma((k + 1)/3) (3^(1/3) z)^k/k! (DLMF 9.12.v; the coefficients by the step
//!   a_(k+3) = a_k z^3/((k + 2)(k + 3))), Hi'(z) = 3^(-1/3)/pi sum_k Gamma((k + 2)/3) (3^(1/3) z)^k/k!, and
//!   Gi = Bi - Hi, Gi' = Bi' - Hi' (DLMF 9.12.12). The series and the difference cancel about
//!   (4/3) |z|^(3/2) log2(e) bits (Hi and Bi grow like exp(2/3 z^(3/2)) while Gi decays like 1/(pi z)), which the
//!   working precision adds.
//! - **Modified Lommel functions** `lommel_t1` = t_(mu,nu)(z) = sum_k z^(mu + 1 + 2k)/prod_(j<=k) ((mu + 2j + 1)^2 -
//!   nu^2), the particular solution of z^2 w'' + z w' - (z^2 + nu^2) w = z^(mu+1) (the series of the Lommel s with
//!   the signs of z^2 positive, DLMF 11.9.3 with z -> i z), and `lommel_t2` = T_(mu,nu)(z) = -t_(mu,nu)(z) +
//!   2^(mu-1) Gamma((mu - nu + 1)/2) Gamma((mu + nu + 1)/2) (I_nu(z) + (sin(mu pi) + sin(nu pi))/pi K_nu(z)), the
//!   solution of the equation with right side -z^(mu+1) whose expansion at infinity is z^(mu-1) sum_k prod_(j<=k)
//!   ((mu - 2j + 1)^2 - nu^2) z^(-2k) (derived from DLMF 11.9.5 with the connection K_nu = pi/2 (I_(-nu) -
//!   I_nu)/sin(nu pi), DLMF 10.27.4).
use crate::cover::{cancel_bits, ev, mag, numeric, parts, sum_series, work};
use mc_api::T;
use mc_arith::num::Rat;
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: Some($e), numeric: None, summary: $s }
    };
}

// ---------------------------------------------------------------- Scorer functions

/// Which Scorer function.
#[derive(Clone, Copy, PartialEq)]
enum Scorer {
    Gi,
    Hi,
    GiPrime,
    HiPrime,
}

/// The value at 0: Gi(0) = Bi(0)/3, Hi(0) = 2 Bi(0)/3, Gi'(0) = Bi'(0)/3, Hi'(0) = 2 Bi'(0)/3 (DLMF 9.12.4-9.12.5,
/// 9.12.21), with Bi(0) = 3^(-1/6)/Gamma(2/3) and Bi'(0) = 3^(1/6)/Gamma(1/3).
fn scorer_zero<'h>(fx: &Fx<'_, '_, '_, 'h>, which: Scorer) -> T<'h> {
    let (c, e, g) = match which {
        Scorer::Gi => (fx.rat(1, 3), fx.rat(-1, 6), fx.rat(2, 3)),
        Scorer::Hi => (fx.rat(2, 3), fx.rat(-1, 6), fx.rat(2, 3)),
        Scorer::GiPrime => (fx.rat(1, 3), fx.rat(1, 6), fx.rat(1, 3)),
        Scorer::HiPrime => (fx.rat(2, 3), fx.rat(1, 6), fx.rat(1, 3)),
    };
    let gam = fx.call("gamma", &[g]);
    fx.mul(&[c, fx.pow(fx.int(3), e), fx.pow(gam, fx.int(-1))])
}

/// Hi(z) (`deriv` false) or Hi'(z) at a working-precision number z, by the Maclaurin series.
fn scorer_hi_series<'h>(fx: &mut Fx<'_, '_, '_, 'h>, z: T<'h>, w: u64, deriv: bool) -> R<'h> {
    let d = crate::cover::digits(w);
    let n = |fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>| -> Res<T<'h>> {
        let di = fx.int(d);
        fx.eval(fx.call("n", &[t, di]))
    };
    let c3 = n(fx, fx.pow(fx.int(3), fx.rat(1, 3)))?;
    let g13 = n(fx, fx.call("gamma", &[fx.rat(1, 3)]))?;
    let g23 = n(fx, fx.call("gamma", &[fx.rat(2, 3)]))?;
    let x = fx.mul(&[c3, z]);
    let x2 = fx.mul(&[x, x]);
    // the first three terms; then a_(k+3) = a_k z^3/((k + 2)(k + 3)) (Hi) or z^3/((k + 1)(k + 3)) (Hi')
    let mut a: Vec<T<'h>> = if deriv {
        vec![g23, x, fx.mul(&[g13, fx.rat(1, 6), x2])]
    } else {
        vec![g13, fx.mul(&[g23, x]), fx.mul(&[fx.rat(1, 2), x2])]
    };
    let z3 = fx.eval(fx.mul(&[z, z, z]))?;
    let n0 = (mag(fx, z).powf(1.5) * 2.0) as usize + 8;
    let s = sum_series(fx, w, n0, |fx, k| {
        let cur = a[k];
        let kk = k as i64;
        let den = if deriv { (kk + 1) * (kk + 3) } else { (kk + 2) * (kk + 3) };
        let next = fx.div(fx.mul(&[cur, z3]), fx.int(den));
        let next = fx.eval(next)?;
        a.push(next);
        Ok(Some(cur))
    })?;
    let Some(s) = s else { return Ok(None) };
    let pi = fx.sym("pi");
    let pre = n(fx, fx.div(fx.pow(fx.int(3), fx.rat(if deriv { -1 } else { -2 }, 3)), pi))?;
    Ok(Some(fx.eval(fx.mul(&[pre, s]))?))
}

fn scorer<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: Scorer) -> R<'h> {
    let z = a[0];
    if fx.is_int(z, 0) {
        return Ok(Some(scorer_zero(fx, which)));
    }
    if let Some(s) = fx.infinity(z) {
        // Gi, Gi' and Hi' -> 0 at both ends of the real line; Hi -> infinity at +infinity, 0 at -infinity
        return Ok(match (which, s) {
            (Scorer::Hi, 0) => Some(fx.inf()),
            (_, 0 | 2) => Some(fx.int(0)),
            _ => None,
        });
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // machine arguments: the double-double kernel (round 29), the ball path when it declines
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        let w = match which {
            Scorer::Gi => crate::fast29::Scorer::Gi,
            Scorer::Hi => crate::fast29::Scorer::Hi,
            Scorer::GiPrime => crate::fast29::Scorer::GiPrime,
            Scorer::HiPrime => crate::fast29::Scorer::HiPrime,
        };
        if let Some(v) = crate::fast29::scorer(ms[0], real, w) {
            // Hi and Hi' beyond the largest double (x > 104): the overflow, as airy_bi(1000.) gives
            if real && v.re == f64::INFINITY {
                return Ok(Some(fx.inf()));
            }
            return Ok(crate::fast29::machine_out(fx, v, real));
        }
    }
    let m = mag(fx, z);
    // the series takes about 2 |z|^(3/2) terms (at 1.9 |z|^(3/2) extra bits): beyond the most terms a series may take
    // it would end in `None` anyway, after setting up constants at that precision (scorer_hi(1e6) needed 6 10^8 digits
    // and did not return): decline at once
    if m.powf(1.5) * 2.0 + 8.0 > crate::cover::MAX_TERMS as f64 {
        return Ok(None);
    }
    let extra = (m.powf(1.5) * (4.0 / 3.0) * std::f64::consts::LOG2_E) as u64 + 16;
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let deriv = matches!(which, Scorer::GiPrime | Scorer::HiPrime);
    let Some(hi) = scorer_hi_series(fx, xs[0], wk.w, deriv)? else { return Ok(None) };
    let v = match which {
        Scorer::Hi | Scorer::HiPrime => hi,
        Scorer::Gi | Scorer::GiPrime => {
            let bi = ev(fx, if deriv { "airy_bi_prime" } else { "airy_bi" }, &[xs[0]])?;
            if fx.num(bi).is_none() {
                return Ok(None);
            }
            fx.eval(fx.sub(bi, hi))?
        }
    };
    Ok(wk.out(fx, v, wk.real))
}

fn scorer_gi<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    scorer(fx, a, Scorer::Gi)
}
fn scorer_hi<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    scorer(fx, a, Scorer::Hi)
}
fn scorer_gi_prime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    scorer(fx, a, Scorer::GiPrime)
}
fn scorer_hi_prime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    scorer(fx, a, Scorer::HiPrime)
}

// ---------------------------------------------------------------- modified Lommel functions

/// The denominators (mu + 2j + 1)^2 - nu^2 of t_(mu,nu): is one of them exactly zero (a pole of every term from
/// that one on)?
fn lommel_pole<'h>(fx: &Fx<'_, '_, '_, 'h>, mu: T<'h>, nu: T<'h>) -> bool {
    // (mu + 1 + 2j) = +-nu for some j >= 0: (-mu - 1 +- nu)/2 a non-negative integer
    let (Some(m), Some(v)) = (fx.rat_of(mu), fx.rat_of(nu)) else { return false };
    [v.clone(), v.neg()].iter().any(|s| {
        let j = s.sub(&m).sub(&Rat::from_i64(1)).mul(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2)));
        j.is_int() && !j.num.neg
    })
}

/// t_(mu,nu)(z) at working-precision numbers: sum_k z^(mu + 1 + 2k)/prod_(j<=k) ((mu + 2j + 1)^2 - nu^2).
fn lommel_t1_series<'h>(fx: &mut Fx<'_, '_, '_, 'h>, mu: T<'h>, nu: T<'h>, z: T<'h>, w: u64) -> R<'h> {
    let z2 = fx.mul(&[z, z]);
    let nu2 = fx.mul(&[nu, nu]);
    let den = |fx: &Fx<'_, '_, '_, 'h>, j: usize| {
        let b = fx.add(&[mu, fx.int(2 * j as i64 + 1)]);
        fx.sub(fx.mul(&[b, b]), nu2)
    };
    let d0 = den(fx, 0);
    if fx.num(d0).is_none_or(|n| n.is_zero()) {
        return Ok(None);
    }
    let zp = crate::cover::evpow(fx, z, fx.add(&[mu, fx.int(1)]))?;
    let mut t = fx.div(zp, d0);
    let n0 = mag(fx, z) as usize + 4;
    sum_series(fx, w, n0, |fx, k| {
        let cur = t;
        let d = den(fx, k + 1);
        if fx.num(d).is_none_or(|n| n.is_zero()) {
            return Ok(None);
        }
        t = fx.div(fx.mul(&[t, z2]), d);
        Ok(Some(cur))
    })
}

fn lommel_t1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (mu, nu, z) = (a[0], a[1], a[2]);
    if lommel_pole(fx, mu, nu) {
        return Ok(Some(fx.sym("complex_infinity")));
    }
    if fx.is_int(z, 0) && fx.f64_of(mu).is_some_and(|m| m > -1.0) {
        return Ok(Some(fx.int(0)));
    }
    if let Some(v) = crate::cover::lommel_exact(fx, a, 2)? {
        return Ok(Some(v));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        if let Some(v) = crate::fast29::lommel_t1(ms[0], ms[1], ms[2]) {
            return Ok(crate::fast29::machine_out(fx, v, real && ms[2].re.hi >= 0.0));
        }
    }
    let extra = cancel_bits(fx, z, 1.0);
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let Some(s) = lommel_t1_series(fx, xs[0], xs[1], xs[2], wk.w)? else { return Ok(None) };
    let real = wk.real && parts(fx, xs[2]).is_some_and(|p| p.0 >= 0.0);
    Ok(wk.out(fx, s, real))
}

/// T_(mu,nu)(z) where a gamma factor of the connection formula has a pole ((mu - nu + 1)/2 or (mu + nu + 1)/2 a
/// non-positive integer) and the expansion at infinity terminates ((mu - 2j + 1)^2 = nu^2 for some j >= 1): the
/// finite sum z^(mu-1) sum_(k<j) prod_(i<=k) ((mu - 2i + 1)^2 - nu^2) z^(-2k), the limit of the formula.
fn lommel_t2_terminating<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> Option<T<'h>> {
    let (m, v) = (fx.rat_of(a[0])?, fx.rat_of(a[1])?);
    let one = Rat::from_i64(1);
    let half = Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2));
    let nonpos_int = |q: &Rat| q.is_int() && (q.num.neg || q.is_zero());
    let pole = nonpos_int(&m.sub(&v).add(&one).mul(&half)) || nonpos_int(&m.add(&v).add(&one).mul(&half));
    if !pole {
        return None;
    }
    let v2 = v.mul(&v);
    let mut c = one.clone();
    let mut terms = Vec::new();
    for k in 0..64i64 {
        terms.push(fx.mul(&[fx.q(&c), fx.pow(a[2], fx.q(&m.sub(&one).sub(&Rat::from_i64(2 * k))))]));
        let b = m.sub(&Rat::from_i64(2 * k + 1));
        let f = b.mul(&b).sub(&v2);
        if f.is_zero() {
            return Some(fx.add(&terms));
        }
        c = c.mul(&f);
    }
    None
}

/// T_(mu,nu)(z) = -t_(mu,nu)(z) + 2^(mu-1) Gamma((mu - nu + 1)/2) Gamma((mu + nu + 1)/2) (I_nu(z) + (sin(mu pi) +
/// sin(nu pi))/pi K_nu(z)).
fn lommel_t2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if let Some(v) = lommel_t2_terminating(fx, a) {
        return Ok(Some(v));
    }
    if let Some(v) = crate::cover::lommel_exact(fx, a, 3)? {
        return Ok(Some(v));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let extra = cancel_bits(fx, a[2], 2.0) + 16;
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let (mu, nu, z) = (xs[0], xs[1], xs[2]);
    // an integer order nu (as a number of another kind) as the exact integer, so that K takes its integer branch
    let nu_e = match fx.f64_of(nu) {
        Some(v) if fx.num(a[1]).is_some_and(|n| matches!(n, mc_arith::Num::Real(_))) && v == v.round() && v.abs() < 1e6 => {
            if crate::cover::has_radius(fx, nu) { nu } else { fx.int(v as i64) }
        }
        _ => nu,
    };
    let Some(t) = lommel_t1_series(fx, mu, nu, z, wk.w)? else { return Ok(None) };
    let half = fx.rat(1, 2);
    let g1 = ev(fx, "gamma", &[fx.mul(&[half, fx.add(&[fx.sub(mu, nu), fx.int(1)])])])?;
    let g2 = ev(fx, "gamma", &[fx.mul(&[half, fx.add(&[mu, nu, fx.int(1)])])])?;
    let p2 = crate::cover::evpow(fx, fx.int(2), fx.sub(mu, fx.int(1)))?;
    let bi = ev(fx, "bessel_i", &[nu_e, z])?;
    let bk = ev(fx, "bessel_k", &[nu_e, z])?;
    let pi = crate::cover::pi_at(fx, wk.w)?;
    let smu = ev(fx, "sin", &[fx.mul(&[mu, pi])])?;
    let snu = ev(fx, "sin", &[fx.mul(&[nu, pi])])?;
    for v in [g1, g2, p2, bi, bk, smu, snu] {
        if fx.num(v).is_none() {
            return Ok(None);
        }
    }
    let c = fx.div(fx.add(&[smu, snu]), pi);
    let v = fx.add(&[fx.neg(t), fx.mul(&[p2, g1, g2, fx.add(&[bi, fx.mul(&[c, bk])])])]);
    let v = fx.eval(v)?;
    let real = wk.real && parts(fx, z).is_some_and(|p| p.0 >= 0.0);
    Ok(wk.out(fx, v, real))
}

// ---------------------------------------------------------------- Carlson symmetric integrals

/// R_F(x, y, z) at working-precision numbers by the duplication algorithm (Carlson 1995, DLMF 19.36.1): x <- (x +
/// lambda)/4 with lambda = sqrt(x) sqrt(y) + sqrt(y) sqrt(z) + sqrt(z) sqrt(x) (principal roots) until the
/// relative spread delta about the mean A is below 2^(-w/8), then A^(-1/2) times the series 1 - E2/10 + E3/14 +
/// E2^2/24 - 3 E2 E3/44 - 5 E2^3/208 + 3 E3^2/104 + E2^2 E3/16, whose omitted terms are O(delta^8).
pub(crate) fn carlson_rf_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>, y: T<'h>, z: T<'h>, w: u64) -> R<'h> {
    let (mut x, mut y, mut z) = (x, y, z);
    let tol = 2f64.powf(-(w as f64) / 8.0);
    for it in 0..(w as usize + 200) {
        if it % 32 == 31 {
            fx.tick(1)?;
        }
        let a = fx.eval(fx.mul(&[fx.add(&[x, y, z]), fx.rat(1, 3)]))?;
        let ma = mag(fx, a);
        if !ma.is_finite() {
            return Ok(None);
        }
        // a zero mean (R_F(-e1, -e2, -e3) of the Weierstrass roots) is left by the first step
        let spread = if ma == 0.0 { f64::INFINITY } else { [x, y, z].iter().map(|&t| mag(fx, fx.sub(a, t))).fold(0.0f64, f64::max) / ma };
        if spread < tol {
            let xx = fx.eval(fx.sub(fx.int(1), fx.div(x, a)))?;
            let yy = fx.eval(fx.sub(fx.int(1), fx.div(y, a)))?;
            let zz = fx.eval(fx.neg(fx.add(&[xx, yy])))?;
            let e2 = fx.eval(fx.sub(fx.mul(&[xx, yy]), fx.mul(&[zz, zz])))?;
            let e3 = fx.eval(fx.mul(&[xx, yy, zz]))?;
            let s = fx.add(&[
                fx.int(1),
                fx.mul(&[fx.rat(-1, 10), e2]),
                fx.mul(&[fx.rat(1, 14), e3]),
                fx.mul(&[fx.rat(1, 24), e2, e2]),
                fx.mul(&[fx.rat(-3, 44), e2, e3]),
                fx.mul(&[fx.rat(-5, 208), e2, e2, e2]),
                fx.mul(&[fx.rat(3, 104), e3, e3]),
                fx.mul(&[fx.rat(1, 16), e2, e2, e3]),
            ]);
            let ra = ev(fx, "sqrt", &[a])?;
            let v = fx.eval(fx.div(s, ra))?;
            return Ok(fx.num(v).map(|_| v));
        }
        let sx = ev(fx, "sqrt", &[x])?;
        let sy = ev(fx, "sqrt", &[y])?;
        let sz = ev(fx, "sqrt", &[z])?;
        let lam = fx.eval(fx.add(&[fx.mul(&[sx, sy]), fx.mul(&[sy, sz]), fx.mul(&[sz, sx])]))?;
        let q = fx.rat(1, 4);
        x = fx.eval(fx.mul(&[fx.add(&[x, lam]), q]))?;
        y = fx.eval(fx.mul(&[fx.add(&[y, lam]), q]))?;
        z = fx.eval(fx.mul(&[fx.add(&[z, lam]), q]))?;
    }
    Ok(None)
}

/// Is the number exactly zero?
fn zero<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    fx.num(t).is_some_and(|n| n.is_zero())
}

fn carlson_rf<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, y, z) = (a[0], a[1], a[2]);
    if a.iter().filter(|&&t| zero(fx, t)).count() >= 2 {
        return Ok(Some(fx.sym("complex_infinity")));
    }
    if x == y && y == z {
        return Ok(Some(fx.pow(x, fx.rat(-1, 2))));
    }
    if !numeric(fx, a) {
        // R_F(0, x, y) = (pi/2) R_K(x, y)
        if let Some(i) = a.iter().position(|&t| zero(fx, t)) {
            let rest: Vec<T<'h>> = a.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &t)| t).collect();
            return Ok(Some(fx.mul(&[fx.rat(1, 2), fx.sym("pi"), fx.call("carlson_rk", &rest)])));
        }
        return Ok(None);
    }
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        if let Some(v) = crate::fast29::carlson_rf_cd(ms[0], ms[1], ms[2]) {
            let real = real && ms.iter().all(|z| z.re.hi >= 0.0);
            return Ok(crate::fast29::machine_out(fx, v.to_c64(), real));
        }
    }
    let Some((wk, xs)) = work(fx, a, 16) else { return Ok(None) };
    let Some(v) = carlson_rf_at(fx, xs[0], xs[1], xs[2], wk.w)? else { return Ok(None) };
    let real = wk.real && a.iter().all(|&t| fx.f64_of(t).is_some_and(|v| v >= 0.0));
    Ok(wk.out(fx, v, real))
}

/// R_K(x, y) = (2/pi) R_F(0, x, y) = 1/AGM(sqrt(x), sqrt(y)): the complete integral, K(m) = (pi/2) R_K(1, 1 - m).
fn carlson_rk<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, y) = (a[0], a[1]);
    if zero(fx, x) || zero(fx, y) {
        return Ok(Some(fx.sym("complex_infinity")));
    }
    if x == y {
        return Ok(Some(fx.pow(x, fx.rat(-1, 2))));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        if let Some(v) = crate::fast29::carlson_rf_cd(crate::cdd::CD::ZERO, ms[0], ms[1]) {
            let v = v.scale(2.0) / crate::cdd::CD::real(crate::dd::PI);
            let real = real && ms.iter().all(|z| z.re.hi >= 0.0);
            return Ok(crate::fast29::machine_out(fx, v.to_c64(), real));
        }
    }
    let Some((wk, xs)) = work(fx, a, 16) else { return Ok(None) };
    let Some(v) = carlson_rf_at(fx, fx.int(0), xs[0], xs[1], wk.w)? else { return Ok(None) };
    let pi = crate::cover::pi_at(fx, wk.w)?;
    let v = fx.eval(fx.div(fx.mul(&[fx.int(2), v]), pi))?;
    let real = wk.real && a.iter().all(|&t| fx.f64_of(t).is_some_and(|v| v >= 0.0));
    Ok(wk.out(fx, v, real))
}

// ---------------------------------------------------------------- Coulomb wave functions

/// `t` evaluated; a value that is not a number (a closed form of exact arguments) then numerically at w bits.
fn nw<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> Res<T<'h>> {
    let v = fx.eval(t)?;
    if fx.num(v).is_some() {
        return Ok(v);
    }
    let d = fx.int(crate::cover::digits(w));
    fx.eval(fx.call("n", &[v, d]))
}

/// Which Coulomb function.
#[derive(Clone, Copy, PartialEq)]
enum Coulomb {
    F,
    G,
    H1,
    H2,
}

/// The Coulomb functions at working-precision numbers l, eta, rho (DLMF 33.2):
/// F = C_l rho^(l+1) e^(-i rho) M(l + 1 - i eta, 2l + 2, 2 i rho) with C_l = 2^l e^(-pi eta/2) |Gamma(l + 1 + i eta)|
/// /Gamma(2l + 2) (33.2.4-33.2.5; |Gamma| continued as exp((lnGamma(l + 1 + i eta) + lnGamma(l + 1 - i eta))/2)),
/// H+ = e^(i theta) (-2 i rho)^(l + 1 + i eta) U(l + 1 + i eta, 2l + 2, -2 i rho) with theta = rho - eta ln(2 rho) -
/// l pi/2 + sigma_l, sigma_l = (lnGamma(l + 1 + i eta) - lnGamma(l + 1 - i eta))/(2 i) (33.2.7, 33.2.10), and G =
/// H+ - i F, H- = G - i F. F is formed from M itself, not from H+ - H-, so a tiny F (eta large) keeps its digits.
fn coulomb_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, l: T<'h>, eta: T<'h>, rho: T<'h>, w: u64, which: Coulomb) -> R<'h> {
    let i = fx.i();
    let ie = fx.mul(&[i, eta]);
    let l1 = fx.add(&[l, fx.int(1)]);
    let lg_p = ev(fx, "log_gamma", &[fx.add(&[l1, ie])])?;
    let lg_m = ev(fx, "log_gamma", &[fx.sub(l1, ie)])?;
    let pi = crate::cover::pi_at(fx, w)?;
    let b = fx.add(&[fx.mul(&[fx.int(2), l]), fx.int(2)]);
    let g2l = nw(fx, fx.call("gamma", &[b]), w)?;
    let p2 = nw(fx, fx.pow(fx.int(2), l), w)?;
    let c_exp = ev(fx, "exp", &[fx.add(&[fx.mul(&[fx.rat(1, 2), fx.add(&[lg_p, lg_m])]), fx.mul(&[fx.rat(-1, 2), pi, eta])])])?;
    let rho_p = crate::cover::evpow(fx, rho, l1)?;
    let e_m = ev(fx, "exp", &[fx.neg(fx.mul(&[i, rho]))])?;
    let m = ev(fx, "hypergeometric_1f1", &[fx.sub(l1, ie), b, fx.mul(&[fx.int(2), i, rho])])?;
    for v in [lg_p, lg_m, g2l, p2, c_exp, rho_p, e_m, m] {
        if fx.num(v).is_none() {
            return Ok(None);
        }
    }
    let f = fx.eval(fx.div(fx.mul(&[p2, c_exp, rho_p, e_m, m]), g2l))?;
    if which == Coulomb::F {
        return Ok(Some(f));
    }
    let sigma = fx.div(fx.sub(lg_p, lg_m), fx.mul(&[fx.int(2), i]));
    let ln2r = ev(fx, "log", &[fx.mul(&[fx.int(2), rho])])?;
    let theta = fx.add(&[rho, fx.neg(fx.mul(&[eta, ln2r])), fx.mul(&[fx.rat(-1, 2), l, pi]), sigma]);
    let e_th = ev(fx, "exp", &[fx.mul(&[i, theta])])?;
    let zz = fx.mul(&[fx.int(-2), i, rho]);
    let pw = crate::cover::evpow(fx, zz, fx.add(&[l1, ie]))?;
    let u = ev(fx, "hypergeometric_u", &[fx.add(&[l1, ie]), b, zz])?;
    for v in [e_th, pw, u, ln2r] {
        if fx.num(v).is_none() {
            return Ok(None);
        }
    }
    let hp = fx.eval(fx.mul(&[e_th, pw, u]))?;
    let if_ = fx.mul(&[i, f]);
    Ok(Some(fx.eval(match which {
        Coulomb::H1 => hp,
        Coulomb::G => fx.sub(hp, if_),
        _ => fx.sub(hp, fx.mul(&[fx.int(2), if_])),
    })?))
}

fn coulomb<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: Coulomb) -> R<'h> {
    let (l, eta, rho) = (a[0], a[1], a[2]);
    if !numeric(fx, a) {
        // eta = 0: the Riccati-Bessel functions (DLMF 33.5.1-33.5.2)
        if fx.is_int(eta, 0) && fx.num(rho).is_none() {
            let r = rho;
            return Ok(Some(match which {
                Coulomb::F => fx.mul(&[r, fx.call("spherical_bessel_j", &[l, r])]),
                Coulomb::G => fx.neg(fx.mul(&[r, fx.call("spherical_bessel_y", &[l, r])])),
                Coulomb::H1 if fx.is_int(l, 0) => fx.call("exp", &[fx.mul(&[fx.i(), r])]),
                Coulomb::H2 if fx.is_int(l, 0) => fx.call("exp", &[fx.neg(fx.mul(&[fx.i(), r]))]),
                Coulomb::H1 => fx.mul(&[fx.i(), r, fx.call("spherical_hankel_h1", &[l, r])]),
                Coulomb::H2 => fx.neg(fx.mul(&[fx.i(), r, fx.call("spherical_hankel_h2", &[l, r])])),
            }));
        }
        return Ok(None);
    }
    // machine reals, l >= 0, rho > 0: Steed's method in double-double
    if let Some((ms, true)) = crate::fast29::machine_args(fx, a) {
        if let Some((f, g)) = crate::fast30::coulomb_fg(ms[0].re.to_f64(), ms[1].re.to_f64(), ms[2].re.to_f64()) {
            use mc_numeric::machine::C64;
            let v = match which {
                Coulomb::F => C64::new(f, 0.0),
                Coulomb::G => C64::new(g, 0.0),
                Coulomb::H1 => C64::new(g, f),
                Coulomb::H2 => C64::new(g, -f),
            };
            return Ok(crate::fast29::machine_out(fx, v, matches!(which, Coulomb::F | Coulomb::G)));
        }
    }
    // the regular function at a complex machine rho: the Maclaurin series in complex double-double numbers (plots of the
    // function over the plane need thousands of values; the ball path below takes about 30 ms each)
    if which == Coulomb::F {
        if let Some((ms, false)) = crate::fast29::machine_args(fx, a) {
            if ms[0].im.hi == 0.0 && ms[1].im.hi == 0.0 {
                if let Some(v) = crate::fast30::coulomb_f_complex(ms[0].re.to_f64(), ms[1].re.to_f64(), ms[2]) {
                    return Ok(crate::fast29::machine_out(fx, v, false));
                }
            }
        }
    }
    let extra = cancel_bits(fx, rho, 2.0) + cancel_bits(fx, eta, std::f64::consts::PI) + 32;
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    // an order l with 2l an integer makes b = 2l + 2 an integer: keep it exact so that U takes its integer branch
    let lt = match fx.f64_of(xs[0]) {
        Some(v) if !crate::cover::has_radius(fx, xs[0]) && (2.0 * v).fract() == 0.0 && v.abs() < 1e6 && fx.num(a[0]).is_some_and(|n| matches!(n, mc_arith::Num::Real(_))) => {
            fx.rat((2.0 * v) as i64, 2)
        }
        _ => xs[0],
    };
    let Some(v) = coulomb_at(fx, lt, xs[1], xs[2], wk.w, which)? else { return Ok(None) };
    let real = wk.real && matches!(which, Coulomb::F | Coulomb::G) && parts(fx, xs[2]).is_some_and(|p| p.0 > 0.0);
    Ok(wk.out(fx, v, real))
}

fn coulomb_f<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    coulomb(fx, a, Coulomb::F)
}
fn coulomb_g<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    coulomb(fx, a, Coulomb::G)
}
fn coulomb_h1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    coulomb(fx, a, Coulomb::H1)
}
fn coulomb_h2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    coulomb(fx, a, Coulomb::H2)
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("scorer_gi", &[1], scorer_gi, "scorer_gi(z): the Scorer function Gi(z) = Bi(z) - Hi(z), a solution of w'' - z w = -1/pi"),
    def!("scorer_hi", &[1], scorer_hi, "scorer_hi(z): the Scorer function Hi(z), the integral of exp(-t^3/3 + z t) over t > 0, over pi"),
    def!("scorer_gi_prime", &[1], scorer_gi_prime, "scorer_gi_prime(z): the derivative of the Scorer function Gi(z)"),
    def!("scorer_hi_prime", &[1], scorer_hi_prime, "scorer_hi_prime(z): the derivative of the Scorer function Hi(z)"),
    def!("lommel_t1", &[3], lommel_t1, "lommel_t1(mu, nu, z): the modified Lommel function t_(mu, nu)(z)"),
    def!("lommel_t2", &[3], lommel_t2, "lommel_t2(mu, nu, z): the modified Lommel function T_(mu, nu)(z)"),
    def!("carlson_rf", &[3], carlson_rf, "carlson_rf(x, y, z): Carlson's symmetric elliptic integral of the first kind R_F(x, y, z)"),
    def!("carlson_rk", &[2], carlson_rk, "carlson_rk(x, y): the complete symmetric integral R_K(x, y) = (2/pi) R_F(0, x, y)"),
    def!("coulomb_f", &[3], coulomb_f, "coulomb_f(l, eta, rho): the regular Coulomb wave function F_l(eta, rho)"),
    def!("coulomb_g", &[3], coulomb_g, "coulomb_g(l, eta, rho): the irregular Coulomb wave function G_l(eta, rho)"),
    def!("coulomb_h1", &[3], coulomb_h1, "coulomb_h1(l, eta, rho): the outgoing Coulomb wave function H+_l(eta, rho) = G + i F"),
    def!("coulomb_h2", &[3], coulomb_h2, "coulomb_h2(l, eta, rho): the incoming Coulomb wave function H-_l(eta, rho) = G - i F"),
];
