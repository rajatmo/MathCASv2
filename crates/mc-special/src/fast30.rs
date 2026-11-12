//! Machine fast paths added in round 30, in double-double arithmetic rounded once.
//!
//! Coulomb wave functions F_l(eta, rho), G_l(eta, rho) for real l >= 0, real eta and rho > 0 by Steed's method
//! (Barnett, Feng, Steed and Goldfarb, Comput. Phys. Commun. 8 (1974) 377; Barnett, CPC 27 (1982) 147; DLMF 33.8):
//! the continued fraction CF1 for f = F'/F, the complex continued fraction CF2 for p + i q = H+'/H+, and the
//! Wronskian G F' - G' F = 1 (DLMF 33.2.12) give F = s sqrt(q/((f - p)^2 + q^2)), G = (f - p) F/q, where the sign s
//! of F_l is that of a downward recurrence started past l (F is the dominant solution downwards). Inside the turning
//! point CF2 converges slowly; past an iteration cap the caller falls back to its ball path.
use crate::cdd::CD;
use crate::dd::DD;

const EPS: f64 = 1e-31;

/// (F, G) at real l >= 0, eta, rho > 0, or `None` when a continued fraction does not converge in time.
pub(crate) fn coulomb_fg(l: f64, eta: f64, rho: f64) -> Option<(f64, f64)> {
    if !(l >= 0.0 && rho > 0.0 && l.is_finite() && eta.is_finite() && rho.is_finite()) || l > 1e4 || rho > 1e5 {
        return None;
    }
    let (ld, ed, rd) = (DD::new(l), DD::new(eta), DD::new(rho));
    // S_k = k/rho + eta/k, R_k^2 = 1 + eta^2/k^2, T_k = S_k + S_(k+1) (DLMF 33.4.1-33.4.4)
    let s = |k: DD| k / rd + ed / k;
    let r2 = |k: DD| DD::ONE + ed.sqr() / k.sqr();
    // CF1: f = S_(l+1) - R_(l+1)^2/(T_(l+1) - R_(l+2)^2/(T_(l+2) - ...)), modified Lentz
    let tiny = DD::new(1e-300);
    let k1 = ld.add_f(1.0);
    let mut f = s(k1);
    if f.hi == 0.0 {
        f = tiny;
    }
    let (mut c, mut d) = (f, DD::ZERO);
    let mut converged = false;
    let mut steps = 0usize;
    for j in 1..200_000usize {
        let k = ld.add_f(j as f64);
        let a = -r2(k);
        let b = s(k) + s(k.add_f(1.0));
        d = b + a * d;
        if d.hi == 0.0 {
            d = tiny;
        }
        c = b + a / c;
        if c.hi == 0.0 {
            c = tiny;
        }
        d = d.recip();
        let delta = c * d;
        f = f * delta;
        steps = j;
        if (delta.add_f(-1.0)).abs().hi < EPS {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }
    // the sign of F_l: a downward recurrence u_(k-1) = (S_k u_k + u'_k)/R_k, u'_(k-1) = S_k u_(k-1) - R_k u_k from
    // m = l + steps + 1 (u_m > 0 there), in machine numbers
    let m = steps + 1;
    let sf = |k: f64| k / rho + eta / k;
    let rf = |k: f64| (1.0 + eta * eta / (k * k)).sqrt();
    let (mut u, mut du) = (1.0f64, sf(l + m as f64 + 1.0));
    for j in (1..=m).rev() {
        let k = l + j as f64;
        let u0 = (sf(k) * u + du) / rf(k);
        du = sf(k) * u0 - rf(k) * u;
        u = u0;
        if u.abs() > 1e250 {
            u *= 1e-250;
            du *= 1e-250;
        }
    }
    let sign = if u < 0.0 { -1.0 } else { 1.0 };
    // CF2: H+'/H+ = i (1 - eta/rho) + (i/rho) a b/(2 (rho - eta + i) + (a + 1)(b + 1)/(2 (rho - eta + 2 i) + ...)),
    // a = 1 + l + i eta, b = -l + i eta
    let a0 = CD::new(ld.add_f(1.0), ed);
    let b0 = CD::new(-ld, ed);
    let rme = rd - ed;
    let tiny_c = CD::real(tiny);
    let mut g = tiny_c;
    let (mut c2, mut d2) = (g, CD::ZERO);
    let mut ok = false;
    for n in 1..100_000usize {
        let nf = (n - 1) as f64;
        let an = (a0 + CD::f(nf)) * (b0 + CD::f(nf));
        let bn = CD::new(rme.mul_f(2.0), DD::new(2.0 * n as f64));
        d2 = bn + an * d2;
        if d2.abs() == 0.0 {
            d2 = tiny_c;
        }
        c2 = bn + an / c2;
        if c2.abs() == 0.0 {
            c2 = tiny_c;
        }
        d2 = CD::ONE / d2;
        let delta = c2 * d2;
        g = g * delta;
        if (delta - CD::ONE).abs() < EPS {
            ok = true;
            break;
        }
    }
    if !ok {
        return None;
    }
    // p + i q = i (1 - eta/rho) + (i/rho) g
    let pq = CD::new(DD::ZERO, DD::ONE - ed / rd) + (g / CD::real(rd)).mul_i();
    let (p, q) = (pq.re, pq.im);
    if q.hi.is_nan() || q.hi <= 0.0 {
        return None;
    }
    let fp = f - p;
    let ff = (q / (fp.sqr() + q.sqr())).sqrt().mul_f(sign);
    let gg = fp * ff / q;
    let (fo, go) = (ff.to_f64(), gg.to_f64());
    // inside the turning point q = Im(H+'/H+) ~ 1/G^2 comes out of a cancellation that costs log10(G^2) of the 32
    // digits: past |G| = 1e6 the ball path is used
    (fo.is_finite() && go.is_finite() && go.abs() < 1e6).then_some((fo, go))
}

/// F_l(eta, rho) at real l >= 0 and eta and a complex machine rho of modulus at most 12 (round 32): the Maclaurin form
/// F = C_l(eta) rho^(l+1) sum_j a_j rho^j with a_0 = 1 and j (j + 2l + 1) a_j = 2 eta a_(j-1) - a_(j-2) (substitute the series into
/// u'' + (1 - 2 eta/rho - l(l+1)/rho^2) u = 0; DLMF 33.6.5), summed in complex double-double numbers, with
/// C_l = 2^l e^(-pi eta/2) |Gamma(l + 1 + i eta)|/Gamma(2l + 2) (DLMF 33.2.5) from the machine log-gamma, which limits
/// |ln C_l| to 3 so that its error stays near one unit in the last place. `None` when the sum cancels more than 14 digits
/// or does not converge; the caller then takes the ball path.
pub(crate) fn coulomb_f_complex(l: f64, eta: f64, rho: crate::cdd::CD) -> Option<mc_numeric::machine::C64> {
    if !((0.0..=200.0).contains(&l) && eta.is_finite() && eta.abs() <= 8.0) || !rho.finite() || rho.abs() > 12.0 || rho.abs() == 0.0 {
        return None;
    }
    let (ld, ed) = (DD::new(l), DD::new(eta));
    let cl = coulomb_c_l(l, ed)?;
    let (mut a_prev, mut a) = (DD::ZERO, DD::ONE);
    let mut pw = CD::ONE;
    let (mut sum, mut big) = (CD::ONE, 1.0f64);
    let mut converged = false;
    for j in 1..600usize {
        let jf = DD::new(j as f64);
        let next = (ed.mul_f(2.0) * a - a_prev) / (jf * (jf + ld.mul_f(2.0) + DD::ONE));
        a_prev = a;
        a = next;
        pw = pw * rho;
        let t = pw.mul_dd(a);
        sum = sum + t;
        big = big.max(t.abs());
        if t.abs() < 1e-33 * big && j as f64 > rho.abs() {
            converged = true;
            break;
        }
    }
    if !converged || !sum.finite() || big > 1e14 * sum.abs() {
        return None;
    }
    let lead = rho.pow(CD::new(ld + DD::ONE, DD::ZERO));
    let v = (lead * sum).mul_dd(cl);
    v.finite().then(|| v.to_c64())
}

/// C_l(eta) = 2^l e^(-pi eta/2) |Gamma(l + 1 + i eta)|/(2l + 1)! (DLMF 33.2.5) in double-double numbers at an integer or
/// half-integer l, where |Gamma|^2 is elementary: pi eta/sinh(pi eta) prod_(s=1)^l (s^2 + eta^2) (integer l; DLMF 5.4.3) or
/// pi/cosh(pi eta) prod_(s=0)^(l-1/2) ((s + 1/2)^2 + eta^2) (half-integer l; DLMF 5.4.4).
fn coulomb_c_l(l: f64, eta: DD) -> Option<DD> {
    if l > 40.0 || (2.0 * l).fract() != 0.0 {
        return None;
    }
    let x = crate::dd::PI * eta;
    let e2 = eta.sqr();
    let mut p = DD::ONE;
    let g2 = if l.fract() == 0.0 {
        for s in 1..=(l as u64) {
            p = p * (DD::new((s * s) as f64) + e2);
        }
        // x/sinh x, by its series near 0
        let sh = if x.abs().hi < 0.5 {
            let (mut t, mut s) = (DD::ONE, DD::ONE);
            for k in 1..30u32 {
                t = t * x.sqr() / DD::new(((2 * k) * (2 * k + 1)) as f64);
                s = s + t;
            }
            s
        } else {
            (crate::dd::exp(x) - crate::dd::exp(-x)) / (x + x)
        };
        p / sh
    } else {
        for s in 0..((l - 0.5) as u64 + 1) {
            let h = DD::new(s as f64 + 0.5);
            p = p * (h.sqr() + e2);
        }
        let ch = (crate::dd::exp(x) + crate::dd::exp(-x)).mul_f(0.5);
        p * crate::dd::PI / ch
    };
    let mut fact = DD::ONE;
    for k in 2..=((2.0 * l) as u64 + 1) {
        fact = fact * DD::new(k as f64);
    }
    let mut two_l = DD::new(2.0).powi(l.floor() as u64);
    if l.fract() != 0.0 {
        two_l = two_l * DD::new(2.0).sqrt();
    }
    Some(two_l * crate::dd::exp(-x.mul_f(0.5)) * g2.sqrt() / fact)
}
