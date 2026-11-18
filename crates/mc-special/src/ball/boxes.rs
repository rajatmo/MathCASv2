//! Complex-ball enclosures by name, for `centered_interval` arguments with complex parts (registered with
//! `mc_numeric::uncertain_cball`): the functions of [`Special`] with a complex-ball kernel, and functions built
//! from them by ball arithmetic, which keeps every enclosure:
//! - Fresnel integrals (DLMF 7.5.7-8 written with erf): S(z) = (1 + i)/4 (erf(w+) - i erf(w-)),
//!   C(z) = (1 - i)/4 (erf(w+) + i erf(w-)), w+- = (1 +- i) sqrt(pi) z/2;
//! - the auxiliary functions (DLMF 7.2.10-11 solved for f, g): f = (1/2 - S) cos(pi z^2/2) - (1/2 - C) sin(pi z^2/2),
//!   g = (1/2 - C) cos(pi z^2/2) + (1/2 - S) sin(pi z^2/2);
//! - Hankel functions H_nu = J_nu +- i Y_nu (DLMF 10.4.3) and the spherical ones sqrt(pi/(2z)) H_(n+1/2)(z) (10.47.5-6);
//! - the Pochhammer symbol Gamma(a + n)/Gamma(a) and the subfactorial Gamma(x + 1, -1)/e (the principal value);
//! - the Coulomb functions F, G, H+, H- by Kummer's functions (the formulas of the point evaluation);
//! - the principal Lambert W off its cut, proved by Krawczyk's operator around the value at the centre;
//! - the modular functions eta, lambda, J and the Weierstrass functions, half-periods and invariants (their own
//!   module, [`super::lattice`], with the mean value form in every argument).
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
        ("coulomb_f" | "coulomb_g" | "coulomb_h1" | "coulomb_h2", [l, eta, rho]) => coulomb(name, l, eta, rho, w)?,
        ("product_log", [z]) => lambert_box(z, w)?,
        _ if super::lattice::has(name) => super::lattice::enclose(name, a, w)?,
        _ => Special::find(name, a.len())?.ball_complex(a, w)?,
    };
    Some(v.round(p))
}

fn f64_of(x: &mc_arith::num::Float) -> f64 {
    mc_arith::num::float::dyadic_to_f64(x.neg, &x.man, x.exp)
}

/// An upper bound of a magnitude as a double.
fn mag_up(m: &mc_arith::num::Mag) -> f64 {
    f64_of(&m.to_float()) * (1.0 + 1e-15)
}

/// The principal Lambert W on a box Z off the cut (-inf, -1/e], proved by Krawczyk's operator: with w0 W at the
/// centre and Y = 1/g'(w0) for g(w) = w e^w - z, K(D) = w0 - Y g(w0) + (1 - Y g'(D)) (D - w0) over the rectangle D
/// around w0 contains every T(w) = w - Y g(w), w in D, z in Z (g(w) - g(w0) = (w - w0) times the mean of g' on the
/// segment, a point of the convex hull of g'(D), which the rectangle g'(D) = e^D (1 + D) contains). K(D) inside D
/// proves for each z one zero of g in D (Krawczyk 1969; Moore, Kearfott and Cloud, "Introduction to Interval
/// Analysis", 8.2, read in the plane as a two-dimensional map). Since W is continuous on Z and W(centre) = w0 lies
/// in D, W(Z) stays in D (leaving it would put a zero on the boundary, but every zero in D lies in K(D)), so W(z) =
/// T(W(z)) lies in K(D), the enclosure returned. D grows from twice the first-order radius |W'| rad(Z).
fn lambert_box(z: &CBall, w: u64) -> Option<CBall> {
    use mc_arith::num::Mag;
    use mc_numeric::machine::C64;
    let (zr, zi) = (f64_of(&z.re.mid), f64_of(&z.im.mid));
    let (rr, ri) = (mag_up(&z.re.rad), mag_up(&z.im.rad));
    // off the cut: the box must not meet the real ray (-inf, -1/e]
    let meets_axis = zi.abs() <= ri;
    if meets_axis && zr - rr <= -0.36788 {
        return None;
    }
    let w0 = crate::more::lambert_w(0, C64::new(zr, zi))?;
    let point = |v: f64| Ball::exact(super::float_of(v));
    // the centre refined by two Newton steps at the working precision (midpoints only)
    let zc = CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()));
    let one = CBall::from_i64(1);
    let mut c = CBall::new(point(w0.re), point(w0.im));
    for _ in 0..3 {
        let e = cball::exp(&c, w)?;
        let f = c.mul(&e, w).sub(&zc, w);
        let d = f.div(&e.mul(&c.add(&one, w), w), w)?;
        let n = c.sub(&d, w);
        c = CBall::new(Ball::exact(n.re.mid), Ball::exact(n.im.mid));
    }
    let ec = cball::exp(&c, w)?;
    let y = ec.mul(&c.add(&one, w), w).recip(w)?;
    // Y g(w0) over the box Z
    let yg = y.mul(&c.mul(&ec, w).sub(z, w), w);
    let dw = mag_up(&z.re.rad).max(mag_up(&z.im.rad)) * 1.5 * (f64_of(&y.re.mid).hypot(f64_of(&y.im.mid)));
    let krawczyk = |d: &CBall| -> Option<CBall> {
        let gd = cball::exp(d, w)?.mul(&d.add(&one, w), w);
        let dev = d.sub(&c, w);
        Some(c.sub(&yg, w).add(&one.sub(&y.mul(&gd, w), w).mul(&dev, w), w))
    };
    let mut r = (2.0 * dw).max(1e-30 * (w0.re.hypot(w0.im) + 1.0));
    for _ in 0..12 {
        let rm = Mag::from_float_up(&super::float_of(r));
        let d = CBall::new(Ball { mid: c.re.mid.clone(), rad: rm }, Ball { mid: c.im.mid.clone(), rad: rm });
        let k = krawczyk(&d)?;
        // K(D) strictly inside D, part by part (the doubles carry a relative margin for their rounding)
        let inside = |kb: &Ball, cm: &mc_arith::num::Float| {
            let off = (f64_of(&kb.mid) - f64_of(cm)).abs() + 1e-15 * f64_of(cm).abs();
            off + mag_up(&kb.rad) < r * (1.0 - 1e-12)
        };
        if inside(&k.re, &c.re.mid) && inside(&k.im, &c.im.mid) {
            // W(Z) lies in K(D), so K(K(D)) encloses it too: a few more steps shrink the second-order part
            let mut best = k;
            for _ in 0..4 {
                let k2 = krawczyk(&best)?;
                let size = |b: &CBall| mag_up(&b.re.rad) + mag_up(&b.im.rad);
                if size(&k2) >= size(&best) {
                    break;
                }
                best = k2;
            }
            return Some(best);
        }
        r *= 2.0;
    }
    None
}

/// The Coulomb functions on boxes, by the formulas of the point evaluation (DLMF 33.2.4-5, 33.2.7, 33.2.10):
/// F = C_l rho^(l+1) e^(-i rho) M(l + 1 - i eta, 2l + 2, 2 i rho) with C_l = 2^l e^(-pi eta/2)
/// exp((lnGamma(l + 1 + i eta) + lnGamma(l + 1 - i eta))/2)/Gamma(2l + 2); H+ = e^(i theta) z^a U(a, b, z) with
/// a = l + 1 + i eta, b = 2l + 2, z = -2 i rho, theta = rho - eta log(2 rho) - l pi/2 + sigma_l,
/// sigma_l = (lnGamma(l + 1 + i eta) - lnGamma(l + 1 - i eta))/(2i); G = H+ - i F, H- = H+ - 2i F. U by Kummer's
/// functions, U = Gamma(1 - b)/Gamma(a - b + 1) M(a, b, z) + Gamma(b - 1)/Gamma(a) z^(1 - b) M(a - b + 1, 2 - b, z)
/// (DLMF 13.2.42), so a box whose b reaches an integer (where Gamma(1 - b) has a pole) has no enclosure here. Every
/// power and logarithm is the principal one, as at a point; a box across a cut gets none (the complex logarithm of a
/// ball across the negative axis is not enclosed).
fn coulomb(name: &str, l: &CBall, eta: &CBall, rho: &CBall, w: u64) -> Option<CBall> {
    let one = CBall::from_i64(1);
    let two = CBall::from_i64(2);
    let pi = Const::Pi.ball(w)?;
    let ie = eta.mul_i();
    let l1 = l.add(&one, w);
    let lgp = Special::LogGamma.ball_complex(&[l1.add(&ie, w)], w)?;
    let lgm = Special::LogGamma.ball_complex(&[l1.sub(&ie, w)], w)?;
    let b = l.mul(&two, w).add(&two, w);
    let half = CBall::real(Ball::from_ratio(1, 2, w));
    let c_exp = cball::exp(&lgp.add(&lgm, w).mul(&half, w).sub(&eta.mul_real(&pi, w).mul(&half, w), w), w)?;
    let c = cball::pow(&two, l, w)?.mul(&c_exp, w).div(&Special::Gamma.ball_complex(std::slice::from_ref(&b), w)?, w)?;
    let f = c
        .mul(&cball::pow(rho, &l1, w)?, w)
        .mul(&cball::exp(&rho.mul_i().neg(), w)?, w)
        .mul(&super::cspecial::hyp1f1(&l1.sub(&ie, w), &b, &rho.mul(&two, w).mul_i(), w)?, w);
    if name == "coulomb_f" {
        return Some(f);
    }
    let a = l1.add(&ie, w);
    let z = rho.mul(&two, w).mul_i().neg();
    let gamma = |x: CBall| Special::Gamma.ball_complex(&[x], w);
    let amb1 = a.sub(&b, w).add(&one, w);
    let m1 = super::cspecial::hyp1f1(&a, &b, &z, w)?;
    let m2 = super::cspecial::hyp1f1(&amb1, &two.sub(&b, w), &z, w)?;
    let t1 = gamma(one.sub(&b, w))?.div(&gamma(amb1.clone())?, w)?.mul(&m1, w);
    let t2 = gamma(b.sub(&one, w))?.div(&gamma(a.clone())?, w)?.mul(&cball::pow(&z, &one.sub(&b, w), w)?, w).mul(&m2, w);
    let u = t1.add(&t2, w);
    let sigma = lgp.sub(&lgm, w).mul(&half, w).div_i();
    let theta = rho
        .sub(&eta.mul(&cball::log(&rho.mul(&two, w), w)?, w), w)
        .sub(&l.mul_real(&pi, w).mul(&half, w), w)
        .add(&sigma, w);
    let hp = cball::exp(&theta.mul_i(), w)?.mul(&cball::pow(&z, &a, w)?, w).mul(&u, w);
    let i_f = f.mul_i();
    Some(match name {
        "coulomb_h1" => hp,
        "coulomb_g" => hp.sub(&i_f, w),
        _ => hp.sub(&i_f.mul(&two, w), w),
    })
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
