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
/// Ei' = e^z/z; P, Q and the Zernike R by the derivative of their 2F1 forms (DLMF 15.5.1); Weber E by
/// 2 E_nu' = E_(nu-1) - E_(nu+1) (DLMF 11.10.36).
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
        ("hypergeometric_1f1", [a, b, z]) => {
            let [_, da, db, dz] = super::cseries::hyp1f1_grad(a, b, z, w)?;
            [da, db, dz][i].clone()
        }
        ("coulomb_f" | "coulomb_g" | "coulomb_h1" | "coulomb_h2", [l, eta, rho]) => coulomb_gradient(name, l, eta, rho, w)?[i].clone(),
        ("legendre_p", [nu, z]) if last => legendre_p_d(nu, &CBall::from_i64(0), z, w)?,
        ("legendre_p", [nu, mu, z]) if last => legendre_p_d(nu, mu, z, w)?,
        ("legendre_q", [nu, z]) if last => legendre_q_d(nu, z, w)?,
        ("zernike_r", [n, m, r]) if last => zernike_d(n, m, r, w)?,
        ("weber_e", [nu, z]) if last => {
            let d = weber_box(&nu.sub(&one, w), z, w)?.sub(&weber_box(&nu.add(&one, w), z, w)?, w);
            d.mul(&half(), w)
        }
        _ => return None,
    })
}

/// A remembered gradient: the function's name, its three arguments, the precision and the partials.
type Gradient = (String, [CBall; 3], u64, [CBall; 3]);

thread_local! {
    /// The last Coulomb gradient: the mean value form asks for the three partials in turn with the same box.
    static COULOMB_GRADIENT: std::cell::RefCell<Option<Gradient>> = const { std::cell::RefCell::new(None) };
}

/// The partial derivatives of a Coulomb function in l, eta and rho, enclosed over the box (one forward pass).
fn coulomb_gradient(name: &str, l: &CBall, eta: &CBall, rho: &CBall, w: u64) -> Option<[CBall; 3]> {
    let key = [l.clone(), eta.clone(), rho.clone()];
    let hit = COULOMB_GRADIENT.with(|c| match &*c.borrow() {
        Some((n, k, p, g)) if n == name && *k == key && *p == w => Some(g.clone()),
        _ => None,
    });
    if hit.is_some() {
        return hit;
    }
    let seed = |j: usize, x: &CBall| {
        let mut d = Du::zero();
        d[j] = CBall::from_i64(1);
        Du { v: x.clone(), d }
    };
    let g = coulomb_du(name, &seed(0, l), &seed(1, eta), &seed(2, rho), w)?.d;
    COULOMB_GRADIENT.with(|c| *c.borrow_mut() = Some((name.to_string(), key, w, g.clone())));
    Some(g)
}

/// A value with its gradient in three directions, both enclosed over the box of the arguments (forward-mode
/// differentiation in ball arithmetic): the derivative part of the mean value form of a function given by a
/// formula, whose own enclosure over the box widens by the dependency of its pieces.
#[derive(Clone)]
struct Du {
    v: CBall,
    d: [CBall; 3],
}

/// x d componentwise.
fn times(d: &[CBall; 3], x: &CBall, w: u64) -> [CBall; 3] {
    [d[0].mul(x, w), d[1].mul(x, w), d[2].mul(x, w)]
}

fn plus(a: &[CBall; 3], b: &[CBall; 3], w: u64) -> [CBall; 3] {
    [a[0].add(&b[0], w), a[1].add(&b[1], w), a[2].add(&b[2], w)]
}

fn is_zero(d: &[CBall; 3]) -> bool {
    d.iter().all(CBall::is_exact_zero)
}

impl Du {
    fn zero() -> [CBall; 3] {
        [CBall::from_i64(0), CBall::from_i64(0), CBall::from_i64(0)]
    }
    fn int(v: i64) -> Du {
        Du { v: CBall::from_i64(v), d: Du::zero() }
    }
    fn add(&self, o: &Du, w: u64) -> Du {
        Du { v: self.v.add(&o.v, w), d: plus(&self.d, &o.d, w) }
    }
    fn sub(&self, o: &Du, w: u64) -> Du {
        Du { v: self.v.sub(&o.v, w), d: plus(&self.d, &times(&o.d, &CBall::from_i64(-1), w), w) }
    }
    fn mul(&self, o: &Du, w: u64) -> Du {
        Du { v: self.v.mul(&o.v, w), d: plus(&times(&self.d, &o.v, w), &times(&o.d, &self.v, w), w) }
    }
    fn div(&self, o: &Du, w: u64) -> Option<Du> {
        let q = self.v.div(&o.v, w)?;
        let r = o.v.recip(w)?;
        let d = times(&plus(&self.d, &times(&o.d, &q.neg(), w), w), &r, w);
        Some(Du { v: q, d })
    }
    fn scale(&self, x: &CBall, w: u64) -> Du {
        Du { v: self.v.mul(x, w), d: times(&self.d, x, w) }
    }
    fn mul_i(&self) -> Du {
        Du { v: self.v.mul_i(), d: [self.d[0].mul_i(), self.d[1].mul_i(), self.d[2].mul_i()] }
    }
    /// f(self) with f(v) = `v` and f'(v) = `dv`.
    fn chain(&self, v: CBall, dv: &CBall, w: u64) -> Du {
        Du { d: times(&self.d, dv, w), v }
    }
    fn exp(&self, w: u64) -> Option<Du> {
        let e = cball::exp(&self.v, w)?;
        Some(self.chain(e.clone(), &e, w))
    }
    fn log(&self, w: u64) -> Option<Du> {
        Some(self.chain(cball::log(&self.v, w)?, &self.v.recip(w)?, w))
    }
    /// The principal power x^y = exp(y log x): d = x^y (y' log x + y x'/x).
    fn pow(&self, y: &Du, w: u64) -> Option<Du> {
        let v = cball::pow(&self.v, &y.v, w)?;
        let mut d = Du::zero();
        if !is_zero(&y.d) {
            d = plus(&d, &times(&y.d, &cball::log(&self.v, w)?, w), w);
        }
        if !is_zero(&self.d) {
            d = plus(&d, &times(&self.d, &y.v.div(&self.v, w)?, w), w);
        }
        Some(Du { d: times(&d, &v, w), v })
    }
    fn log_gamma(&self, w: u64) -> Option<Du> {
        let v = Special::LogGamma.ball_complex(std::slice::from_ref(&self.v), w)?;
        Some(self.chain(v, &super::cseries::digamma_at(&self.v, w)?, w))
    }
    fn gamma(&self, w: u64) -> Option<Du> {
        let v = Special::Gamma.ball_complex(std::slice::from_ref(&self.v), w)?;
        let dv = v.mul(&super::cseries::digamma_at(&self.v, w)?, w);
        Some(self.chain(v, &dv, w))
    }
    fn hyp1f1(a: &Du, b: &Du, z: &Du, w: u64) -> Option<Du> {
        let [v, da, db, dz] = super::cseries::hyp1f1_grad(&a.v, &b.v, &z.v, w)?;
        let d = plus(&plus(&times(&a.d, &da, w), &times(&b.d, &db, w), w), &times(&z.d, &dz, w), w);
        Some(Du { v, d })
    }
}

/// The formula of [`coulomb`] in dual numbers: the value and the derivative in the direction seeded in l, eta, rho.
fn coulomb_du(name: &str, l: &Du, eta: &Du, rho: &Du, w: u64) -> Option<Du> {
    let (one, two) = (Du::int(1), Du::int(2));
    let pi = CBall::real(Const::Pi.ball(w)?);
    let half = CBall::real(Ball::from_ratio(1, 2, w));
    let ie = eta.mul_i();
    let l1 = l.add(&one, w);
    let lgp = l1.add(&ie, w).log_gamma(w)?;
    let lgm = l1.sub(&ie, w).log_gamma(w)?;
    let b = l.scale(&CBall::from_i64(2), w).add(&two, w);
    let c_exp = lgp.add(&lgm, w).scale(&half, w).sub(&eta.scale(&pi, w).scale(&half, w), w).exp(w)?;
    let c = two.pow(l, w)?.mul(&c_exp, w).div(&b.gamma(w)?, w)?;
    let f = c
        .mul(&rho.pow(&l1, w)?, w)
        .mul(&rho.mul_i().scale(&CBall::from_i64(-1), w).exp(w)?, w)
        .mul(&Du::hyp1f1(&l1.sub(&ie, w), &b, &rho.scale(&CBall::from_i64(2), w).mul_i(), w)?, w);
    if name == "coulomb_f" {
        return Some(f);
    }
    let a = l1.add(&ie, w);
    let z = rho.scale(&CBall::from_i64(2), w).mul_i().scale(&CBall::from_i64(-1), w);
    let amb1 = a.sub(&b, w).add(&one, w);
    let m1 = Du::hyp1f1(&a, &b, &z, w)?;
    let m2 = Du::hyp1f1(&amb1, &two.sub(&b, w), &z, w)?;
    let t1 = one.sub(&b, w).gamma(w)?.div(&amb1.gamma(w)?, w)?.mul(&m1, w);
    let t2 = b.sub(&one, w).gamma(w)?.div(&a.gamma(w)?, w)?.mul(&z.pow(&one.sub(&b, w), w)?, w).mul(&m2, w);
    let u = t1.add(&t2, w);
    let sigma = lgp.sub(&lgm, w).scale(&half.mul_i().neg(), w);
    let theta = rho
        .sub(&eta.mul(&rho.scale(&CBall::from_i64(2), w).log(w)?, w), w)
        .sub(&l.scale(&pi, w).scale(&half, w), w)
        .add(&sigma, w);
    let hp = theta.mul_i().exp(w)?.mul(&z.pow(&a, w)?, w).mul(&u, w);
    let i_f = f.mul_i();
    Some(match name {
        "coulomb_h1" => hp,
        "coulomb_g" => hp.sub(&i_f, w),
        _ => hp.sub(&i_f.scale(&CBall::from_i64(2), w), w),
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
        ("legendre_p", [nu, z]) => legendre_p_box(nu, &CBall::from_i64(0), z, w)?,
        ("legendre_p", [nu, mu, z]) => legendre_p_box(nu, mu, z, w)?,
        ("legendre_q", [nu, z]) => legendre_q_box(nu, z, w)?,
        ("zernike_r", [n, m, r]) => zernike_box(n, m, r, w)?,
        ("weber_e", [nu, z]) => weber_box(nu, z, w)?,
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

/// A real box or a point: the complex-ball 2F1 of a box off the real line costs seconds (its series refined at
/// growing precision), so the 2F1 forms below take real spreads only.
fn real_or_point(z: &CBall) -> bool {
    z.is_real() || (z.re.rad.is_zero() && z.im.rad.is_zero())
}

/// d/dz of [`legendre_p_box`]: with A = (1 + z)^(mu/2), B = (1 - z)^(-mu/2), F = 2F1(-nu, nu + 1; 1 - mu; (1 - z)/2),
/// (A B F)' = A B (mu/2) (1/(1 + z) + 1/(1 - z)) F - A B (1/2) (ab/c) 2F1(a + 1, b + 1; c + 1; (1 - z)/2)
/// (DLMF 15.5.1), all over Gamma(1 - mu).
fn legendre_p_d(nu: &CBall, mu: &CBall, z: &CBall, w: u64) -> Option<CBall> {
    if !real_or_point(z) {
        return None;
    }
    let one = CBall::from_i64(1);
    let half = Ball::from_ratio(1, 2, w);
    let c = one.sub(mu, w);
    if super::cseries::nonpos_int(&c).is_some() {
        return None;
    }
    let (a, b) = (nu.neg(), nu.add(&one, w));
    let x = one.sub(z, w).mul_real(&half, w);
    let rg = super::cseries::rgamma(&c, w)?;
    let f1 = super::cspecial::hyp2f1(&a.add(&one, w), &b.add(&one, w), &c.add(&one, w), &x, w)?;
    let mut d = a.mul(&b, w).div(&c, w)?.mul(&f1, w).mul_real(&half, w).neg();
    if mu.is_exact_zero() {
        return Some(d.mul(&rg, w));
    }
    let f = super::cspecial::hyp2f1(&a, &b, &c, &x, w)?;
    let hm = mu.mul_real(&half, w);
    let lp = one.add(z, w).recip(w)?.add(&one.sub(z, w).recip(w)?, w);
    d = d.add(&hm.mul(&lp, w).mul(&f, w), w);
    let ab = cball::pow(&one.add(z, w), &hm, w)?.mul(&cball::pow(&one.sub(z, w), &hm.neg(), w)?, w);
    Some(ab.mul(&d, w).mul(&rg, w))
}

/// d/dz of [`legendre_q_box`]: pi/(2 sin(nu pi)) (cos(nu pi) P'(z) + P'(-z)).
fn legendre_q_d(nu: &CBall, z: &CBall, w: u64) -> Option<CBall> {
    let pi = CBall::real(Const::Pi.ball(w)?);
    let (s, c) = cball::sin_cos(&nu.mul(&pi, w), w)?;
    let zero = CBall::from_i64(0);
    let p = legendre_p_d(nu, &zero, z, w)?;
    let q = legendre_p_d(nu, &zero, &z.neg(), w)?;
    Some(pi.div(&s.mul(&CBall::from_i64(2), w), w)?.mul(&c.mul(&p, w).add(&q, w), w))
}

/// d/dr of [`zernike_box`]: cos(k pi) binomial(s, k) (m r^(m-1) F + 2 r^(m+1) (ab/c) 2F1(a + 1, b + 1; c + 1; r^2))
/// with F = 2F1(a, b; c; r^2), a = -k, b = s + 1, c = m + 1 (DLMF 15.5.1).
fn zernike_d(n: &CBall, m: &CBall, r: &CBall, w: u64) -> Option<CBall> {
    if !real_or_point(r) {
        return None;
    }
    let one = CBall::from_i64(1);
    let half = Ball::from_ratio(1, 2, w);
    let k = n.sub(m, w).mul_real(&half, w);
    let s = n.add(m, w).mul_real(&half, w);
    let m1 = m.add(&one, w);
    if super::cseries::nonpos_int(&m1).is_some() {
        return None;
    }
    let pi = CBall::real(Const::Pi.ball(w)?);
    let cosk = cball::cos(&k.mul(&pi, w), w)?;
    let g = Special::Gamma.ball_complex(&[s.add(&one, w)], w)?;
    let binom = g.mul(&super::cseries::rgamma(&k.add(&one, w), w)?, w).mul(&super::cseries::rgamma(&m1, w)?, w);
    let (a, b, r2) = (k.neg(), s.add(&one, w), r.square(w));
    let f = super::cspecial::hyp2f1(&a, &b, &m1, &r2, w)?;
    let f1 = super::cspecial::hyp2f1(&a.add(&one, w), &b.add(&one, w), &m1.add(&one, w), &r2, w)?;
    let t1 = m.mul(&cball::pow(r, &m.sub(&one, w), w)?, w).mul(&f, w);
    let t2 = cball::pow(r, &m.add(&one, w), w)?.mul(&a.mul(&b, w).div(&m1, w)?, w).mul(&f1, w).mul(&CBall::from_i64(2), w);
    Some(cosk.mul(&binom, w).mul(&t1.add(&t2, w), w))
}

/// P_nu^mu(z) of type 2 (the function of the point evaluation): (1 + z)^(mu/2) (1 - z)^(-mu/2)
/// 2F1~(-nu, nu + 1; 1 - mu; (1 - z)/2) (DLMF 14.3.1); on the cut z > 1 the principal branch of (1 - z)^(-mu/2) and
/// the 2F1 from below, as at a point. `None` where 1 - mu is a pole of Gamma (the regularized limit).
fn legendre_p_box(nu: &CBall, mu: &CBall, z: &CBall, w: u64) -> Option<CBall> {
    if !real_or_point(z) {
        return None;
    }
    let one = CBall::from_i64(1);
    let c = one.sub(mu, w);
    if super::cseries::nonpos_int(&c).is_some() {
        return None;
    }
    let x = one.sub(z, w).mul_real(&Ball::from_ratio(1, 2, w), w);
    let f = super::cspecial::hyp2f1(&nu.neg(), &nu.add(&one, w), &c, &x, w)?.mul(&super::cseries::rgamma(&c, w)?, w);
    if mu.is_exact_zero() {
        return Some(f);
    }
    let hm = mu.mul_real(&Ball::from_ratio(1, 2, w), w);
    let a = cball::pow(&one.add(z, w), &hm, w)?;
    let b = cball::pow(&one.sub(z, w), &hm.neg(), w)?;
    Some(a.mul(&b, w).mul(&f, w))
}

/// Q_nu(z) = pi/(2 sin(nu pi)) (cos(nu pi) P_nu(z) - P_nu(-z)) (DLMF 14.9.10 at mu = 0), the combination of the
/// point evaluation: an identity of the functions analytic off (-inf, -1] and [1, inf), and on the cuts the
/// combination of the limits taken there. `None` for a box that holds -1 or 1, or where sin(nu pi) may vanish.
fn legendre_q_box(nu: &CBall, z: &CBall, w: u64) -> Option<CBall> {
    let one = CBall::from_i64(1);
    if one.sub(z, w).contains_zero() || one.add(z, w).contains_zero() {
        return None;
    }
    let pi = CBall::real(Const::Pi.ball(w)?);
    let (s, c) = cball::sin_cos(&nu.mul(&pi, w), w)?;
    if s.contains_zero() {
        return None;
    }
    let zero = CBall::from_i64(0);
    let p = legendre_p_box(nu, &zero, z, w)?;
    let q = legendre_p_box(nu, &zero, &z.neg(), w)?;
    Some(pi.div(&s.mul(&CBall::from_i64(2), w), w)?.mul(&c.mul(&p, w).sub(&q, w), w))
}

/// R_n^m(r) for any numbers, as at a point: cos(k pi) r^m binomial(s, k) 2F1(-k, s + 1; m + 1; r^2) with
/// k = (n - m)/2, s = (n + m)/2 (the Jacobi form, Born and Wolf 9.2.1, appendix VII; DLMF 18.5.7), binomial(s, k) =
/// Gamma(s + 1)/(Gamma(k + 1) Gamma(m + 1)).
fn zernike_box(n: &CBall, m: &CBall, r: &CBall, w: u64) -> Option<CBall> {
    if !real_or_point(r) {
        return None;
    }
    let one = CBall::from_i64(1);
    let half = Ball::from_ratio(1, 2, w);
    let k = n.sub(m, w).mul_real(&half, w);
    let s = n.add(m, w).mul_real(&half, w);
    let m1 = m.add(&one, w);
    if super::cseries::nonpos_int(&m1).is_some() {
        return None;
    }
    let pi = CBall::real(Const::Pi.ball(w)?);
    let cosk = cball::cos(&k.mul(&pi, w), w)?;
    let g = Special::Gamma.ball_complex(&[s.add(&one, w)], w)?;
    let binom = g.mul(&super::cseries::rgamma(&k.add(&one, w), w)?, w).mul(&super::cseries::rgamma(&m1, w)?, w);
    let f = super::cspecial::hyp2f1(&k.neg(), &s.add(&one, w), &m1, &r.square(w), w)?;
    Some(cosk.mul(&cball::pow(r, m, w)?, w).mul(&binom, w).mul(&f, w))
}

/// The Weber function E_nu(z) = sin(nu pi/2) S1 - cos(nu pi/2) S2 (DLMF 11.10.5) with the entire series
/// S1 = sum (-1)^k (z/2)^(2k)/(Gamma(k + 1 + nu/2) Gamma(k + 1 - nu/2)) and
/// S2 = sum (-1)^k (z/2)^(2k+1)/(Gamma(k + 3/2 + nu/2) Gamma(k + 3/2 - nu/2)) (DLMF 11.10.8-9), each a 1F2 in
/// -z^2/4 over the Gamma functions of its first term. `None` where a lower parameter is a pole (an even or odd
/// integer order, where the series' first terms vanish and the closed forms apply).
fn weber_box(nu: &CBall, z: &CBall, w: u64) -> Option<CBall> {
    // the alternating series cancels about |z| log2(e) bits: beyond |z| = 24 the enclosure costs seconds
    if super::cseries::log2_up(z) > 4.6 {
        return None;
    }
    let one = CBall::from_i64(1);
    let half = Ball::from_ratio(1, 2, w);
    let hn = nu.mul_real(&half, w);
    let t = z.square(w).mul_real(&Ball::from_ratio(-1, 4, w), w);
    let series = |b1: CBall, b2: CBall| -> Option<CBall> {
        let r = super::cseries::rgamma(&b1, w)?.mul(&super::cseries::rgamma(&b2, w)?, w);
        Some(super::cspecial::hyp_pfq(std::slice::from_ref(&one), &[b1, b2], &t, w)?.mul(&r, w))
    };
    let s1 = series(one.add(&hn, w), one.sub(&hn, w))?;
    let th = CBall::real(Ball::from_ratio(3, 2, w));
    let s2 = series(th.add(&hn, w), th.sub(&hn, w))?.mul(&z.mul_real(&half, w), w);
    let pi = CBall::real(Const::Pi.ball(w)?);
    let (sn, cs) = cball::sin_cos(&hn.mul(&pi, w), w)?;
    Some(sn.mul(&s1, w).sub(&cs.mul(&s2, w), w))
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
