//! Complex-ball enclosures by name, for `centered_interval` arguments with complex parts (registered with
//! `mc_numeric::uncertain_cball`): the functions of [`Special`] with a complex-ball kernel, and functions built
//! from them by ball arithmetic, which keeps every enclosure:
//! - Fresnel integrals (DLMF 7.5.7-8 written with erf): S(z) = (1 + i)/4 (erf(w+) - i erf(w-)),
//!   C(z) = (1 - i)/4 (erf(w+) + i erf(w-)), w+- = (1 +- i) sqrt(pi) z/2;
//! - the auxiliary functions (DLMF 7.2.10-11 solved for f, g): f = (1/2 - S) cos(pi z^2/2) - (1/2 - C) sin(pi z^2/2),
//!   g = (1/2 - C) cos(pi z^2/2) + (1/2 - S) sin(pi z^2/2);
//! - Hankel functions H_nu = J_nu +- i Y_nu (DLMF 10.4.3) and the spherical ones sqrt(pi/(2z)) H_(n+1/2)(z) (10.47.5-6);
//! - the Pochhammer symbol Gamma(a + n)/Gamma(a) and the subfactorial Gamma(x + 1, -1)/e (the principal value).
use super::cseries::{bessel_j, bessel_y, int_of};
use super::cspecial::{bessel_yk_int, gamma_inc};
use crate::functions::Special;
use mc_arith::num::Ball;
use mc_numeric::cball::{self, CBall};
use mc_numeric::consts::Const;

/// The enclosure of `name(a...)` (`deriv` None) or of its derivative in the argument `deriv` at precision p, `None`
/// for a name without one or where none is proved. The derivatives (in the last argument unless stated):
/// S' = sin(pi z^2/2), C' = cos(pi z^2/2), f' = -pi z g, g' = pi z f - 1 (DLMF 7.10.1-2, 7.10.4-5 in f, g form);
/// H_nu' = H_(nu-1) - (nu/z) H_nu, h_n' = h_(n-1) - ((n + 1)/z) h_n (10.6.2, 10.51.2); J, Y by (C_(nu-1) - C_(nu+1))/2,
/// I by (I_(nu-1) + I_(nu+1))/2, K by -(K_(nu-1) + K_(nu+1))/2 (10.6.1, 10.29.1); d/dz Gamma(a, z) = -z^(a-1) e^-z
/// (8.8.13), Q likewise over Gamma(a); erf' = 2/sqrt(pi) e^(-z^2), erfi' = 2/sqrt(pi) e^(z^2); Gamma' = Gamma psi;
/// (x)_n in x: (x)_n (psi(x + n) - psi(x)); K'(m) = (E - (1 - m) K)/(2 m (1 - m)), E'(m) = (E - K)/(2m) (19.4.1);
/// Ei' = e^z/z.
pub fn complex_box_d(name: &str, a: &[CBall], deriv: Option<usize>, p: u64) -> Option<CBall> {
    match deriv {
        None => complex_box(name, a, p),
        Some(i) => derivative(name, a, i, p + 32).map(|v| v.round(p)),
    }
}

fn derivative(name: &str, a: &[CBall], i: usize, w: u64) -> Option<CBall> {
    let last = i + 1 == a.len();
    let half = || CBall::real(Ball::from_ratio(1, 2, w));
    let one = CBall::from_i64(1);
    let two_over_sqrt_pi = || -> Option<CBall> { Some(CBall::real(Ball::from_i64(2).div(&Const::Pi.ball(w)?.sqrt(w)?, w)?)) };
    let call = |n: &str, args: &[CBall]| complex_box(n, args, w);
    Some(match (name, a) {
        ("fresnel_s" | "fresnel_c", [z]) => {
            let ang = z.square(w).mul_real(&Const::Pi.ball(w)?, w).mul(&half(), w);
            let (s, c) = cball::sin_cos(&ang, w)?;
            if name == "fresnel_s" {
                s
            } else {
                c
            }
        }
        ("fresnel_f", [z]) => {
            let g = call("fresnel_g", a)?;
            g.mul(z, w).mul_real(&Const::Pi.ball(w)?, w).neg()
        }
        ("fresnel_g", [z]) => {
            let f = call("fresnel_f", a)?;
            f.mul(z, w).mul_real(&Const::Pi.ball(w)?, w).sub(&one, w)
        }
        ("hankel_h1" | "hankel_h2" | "bessel_j" | "bessel_y" | "bessel_i" | "bessel_k", [nu, z]) if last => {
            let lo = call(name, &[nu.sub(&one, w), z.clone()])?;
            match name {
                "hankel_h1" | "hankel_h2" => lo.sub(&nu.div(z, w)?.mul(&call(name, a)?, w), w),
                _ => {
                    let hi = call(name, &[nu.add(&one, w), z.clone()])?;
                    let v = match name {
                        "bessel_i" => lo.add(&hi, w),
                        "bessel_k" => lo.add(&hi, w).neg(),
                        _ => lo.sub(&hi, w),
                    };
                    v.mul(&half(), w)
                }
            }
        }
        ("spherical_hankel_h1" | "spherical_hankel_h2", [n, z]) if last => {
            let lo = call(name, &[n.sub(&one, w), z.clone()])?;
            lo.sub(&n.add(&one, w).div(z, w)?.mul(&call(name, a)?, w), w)
        }
        ("gamma" | "gamma_regularized", [s, z]) if last => {
            let zs = cball::pow(z, &s.sub(&one, w), w)?;
            let v = zs.mul(&cball::exp(&z.neg(), w)?, w).neg();
            if name == "gamma" {
                v
            } else {
                v.div(&Special::Gamma.ball_complex(std::slice::from_ref(s), w)?, w)?
            }
        }
        ("erf" | "erfc" | "erfi", [z]) => {
            let z2 = z.square(w);
            let e = cball::exp(&if name == "erfi" { z2 } else { z2.neg() }, w)?;
            let v = e.mul(&two_over_sqrt_pi()?, w);
            if name == "erfc" {
                v.neg()
            } else {
                v
            }
        }
        ("gamma", [_]) => call("gamma", a)?.mul(&Special::Digamma.ball_complex(a, w)?, w),
        ("pochhammer", [x, n]) if i == 0 => {
            let d = Special::Digamma.ball_complex(&[x.add(n, w)], w)?.sub(&Special::Digamma.ball_complex(std::slice::from_ref(x), w)?, w);
            call("pochhammer", a)?.mul(&d, w)
        }
        ("elliptic_k" | "elliptic_e", [m]) => {
            let (k, e) = (Special::EllipticK.ball_complex(a, w)?, Special::EllipticE.ball_complex(a, w)?);
            let two_m = m.mul(&CBall::from_i64(2), w);
            if name == "elliptic_k" {
                let om = one.sub(m, w);
                e.sub(&om.mul(&k, w), w).div(&two_m.mul(&om, w), w)?
            } else {
                e.sub(&k, w).div(&two_m, w)?
            }
        }
        ("exp_integral_ei", [z]) => cball::exp(z, w)?.div(z, w)?,
        _ => return None,
    })
}

/// The enclosure of `name(a...)` at precision p, `None` for a name without one or where none is proved.
pub fn complex_box(name: &str, a: &[CBall], p: u64) -> Option<CBall> {
    let w = p + 32;
    let v = match (name, a) {
        ("fresnel_s", [z]) => fresnel(z, true, w)?,
        ("fresnel_c", [z]) => fresnel(z, false, w)?,
        ("fresnel_f" | "fresnel_g", [z]) => {
            let (s, c) = (fresnel(z, true, w)?, fresnel(z, false, w)?);
            let half = CBall::real(Ball::from_ratio(1, 2, w));
            let (hs, hc) = (half.sub(&s, w), half.sub(&c, w));
            let pi = Const::Pi.ball(w)?;
            let ang = z.square(w).mul_real(&pi, w).mul_real(&Ball::from_ratio(1, 2, w), w);
            let (sn, cs) = cball::sin_cos(&ang, w)?;
            if name == "fresnel_f" {
                hs.mul(&cs, w).sub(&hc.mul(&sn, w), w)
            } else {
                hc.mul(&cs, w).add(&hs.mul(&sn, w), w)
            }
        }
        ("hankel_h1" | "hankel_h2", [nu, z]) => hankel(nu, z, name == "hankel_h1", w)?,
        ("spherical_hankel_h1" | "spherical_hankel_h2", [n, z]) => {
            let nu = n.add(&CBall::real(Ball::from_ratio(1, 2, w)), w);
            let h = hankel(&nu, z, name == "spherical_hankel_h1", w)?;
            let pi = CBall::real(Const::Pi.ball(w)?);
            let f = cball::sqrt(&pi.div(&z.mul(&CBall::from_i64(2), w), w)?, w)?;
            f.mul(&h, w)
        }
        ("pochhammer", [x, n]) => {
            let g1 = Special::Gamma.ball_complex(&[x.add(n, w)], w)?;
            let g0 = Special::Gamma.ball_complex(std::slice::from_ref(x), w)?;
            g1.div(&g0, w)?
        }
        ("subfactorial", [x]) => {
            let g = gamma_inc(&x.add(&CBall::from_i64(1), w), &CBall::from_i64(-1), w)?;
            let e = cball::exp(&CBall::from_i64(-1), w)?;
            g.mul(&e, w)
        }
        _ => Special::find(name, a.len())?.ball_complex(a, w)?,
    };
    Some(v.round(p))
}

/// S(z) (`sine`) or C(z) by the error function.
fn fresnel(z: &CBall, sine: bool, w: u64) -> Option<CBall> {
    let sp = Const::Pi.ball(w)?.sqrt(w)?;
    let hz = z.mul_real(&sp, w).mul_real(&Ball::from_ratio(1, 2, w), w);
    let up = super::complex::erf(&hz.add(&hz.mul_i(), w), w)?;
    let dn = super::complex::erf(&hz.sub(&hz.mul_i(), w), w)?;
    let q = CBall::real(Ball::from_ratio(1, 4, w));
    let one = CBall::from_i64(1);
    Some(if sine {
        q.mul(&one.add(&CBall::i(), w), w).mul(&up.sub(&dn.mul_i(), w), w)
    } else {
        q.mul(&one.sub(&CBall::i(), w), w).mul(&up.add(&dn.mul_i(), w), w)
    })
}

/// H^(1)_nu(z) (`first`) or H^(2)_nu(z).
fn hankel(nu: &CBall, z: &CBall, first: bool, w: u64) -> Option<CBall> {
    let j = bessel_j(nu, z, w)?;
    let y = match int_of(nu) {
        Some(n) => bessel_yk_int(0, n, z, w)?,
        None => bessel_y(nu, z, w)?,
    };
    let iy = y.mul_i();
    Some(if first { j.add(&iy, w) } else { j.sub(&iy, w) })
}
