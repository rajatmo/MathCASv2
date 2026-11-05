//! Exact values of the package-3C functions (decisions SF.1, SF.2, 3C.2): closed forms at exact special arguments,
//! some also for symbolic arguments; poles give [`Exact::Pole`]; everything else stays unevaluated for `n`.
//!
//! | function | closed forms |
//! |---|---|
//! | erf, erfi, sin_integral, sinh_integral, inverse_erf | 0 at 0; inverse_erf(±1) a pole |
//! | erfc, inverse_erfc | erfc(0) = 1; inverse_erfc(1) = 0, inverse_erfc(0), inverse_erfc(2) poles |
//! | gamma(a, x) | gamma(a, 0) = gamma(a) (a > 0); gamma(1, x) = e^-x; gamma(n, x) = (n-1)! e^-x sum_{k<n} x^k/k! (n <= 20); gamma(1/2, x) = sqrt(pi) erfc(sqrt x) |
//! | gamma_regularized | 1 at x = 0 (a > 0); e^-x at a = 1; e^-x sum_{k<n} x^k/k! (n <= 20) |
//! | beta(z, a, b), beta_regularized | 0 at z = 0; beta(a, b) and 1 at z = 1 |
//! | exp_integral_ei, cos_integral, cosh_integral, log_integral | poles at 0 (log_integral: 0 at 0, a pole at 1) |
//! | exp_integral_e | E_0(x) = e^-x/x; E_n(0) = 1/(n-1) for n > 1, a pole otherwise |
//! | polylog | Li_s(0) = 0; Li_1(z) = -log(1-z); Li_0(z) = z/(1-z); Li_-1(z) = z/(1-z)^2; Li_s(1) = zeta(s) (s > 1); Li_s(-1) = (2^(1-s) - 1) zeta(s); Li_2(1/2) = pi^2/12 - log(2)^2/2 |
//! | hypergeometric | 1 at z = 0; 1F1(a; a; z) = e^z; 2F1(a, b; c; 1) = Gauss's product of gammas (c - a - b > 0) |
//! | bessel_j, bessel_y | J_0(0) = 1, J_nu(0) = 0 (nu > 0 or a non-zero integer); Y_nu(0) a pole (integer nu); orders ±1/2 by sin and cos |
//! | airy | Ai(0), Bi(0), Ai'(0), Bi'(0) through gamma(1/3), gamma(2/3) |
//! | elliptic | K(0) = E(0) = pi/2, K(1) a pole, E(1) = 1; F(0, m) = E(0, m) = 0, F(phi, 0) = E(phi, 0) = phi; Pi(0, m) = K(m) |
//! | jacobi | sn(0) = 0, cn(0) = dn(0) = 1; m = 0: sin, cos, 1; m = 1: tanh, sech, sech; inverse_jacobi_sn(0, m) = 0 |
use crate::exact::Exact;
use crate::functions::Special;
use mc_arith::num::{Int, Rat};
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, T};

use Special as S;

/// The exact rules of the package-3C functions.
pub struct More<'a, 'h> {
    h: &'h Heap,
    ar: &'a Arith<'h, Heap>,
}

fn half() -> Rat {
    Rat::new(Int::from_i64(1), Int::from_i64(2))
}

impl<'a, 'h> More<'a, 'h> {
    /// Rules building with `ar`.
    pub fn new(h: &'h Heap, ar: &'a Arith<'h, Heap>) -> More<'a, 'h> {
        More { h, ar }
    }

    fn rat(&self, t: T<'h>) -> Option<Rat> {
        self.ar.num(t)?.as_rat().cloned()
    }
    fn is(&self, t: T<'h>, v: i64) -> bool {
        self.rat(t).is_some_and(|q| q.cmp_value(&Rat::from_i64(v)).is_eq())
    }
    fn q(&self, q: &Rat) -> T<'h> {
        self.ar.number(&Num::rat(q.clone()))
    }
    fn n(&self, v: i64) -> T<'h> {
        self.ar.int(v)
    }
    fn sym(&self, s: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(s))
    }
    fn call(&self, f: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(f), args)
    }
    fn val(&self, t: T<'h>) -> Option<Exact<'h>> {
        Some(Exact::Value(t))
    }
    fn exp_neg(&self, x: T<'h>) -> T<'h> {
        self.call("exp", &[self.ar.neg(x)])
    }
    /// e^-x sum_{k<n} x^k/k!, times `scale`.
    fn exp_poly(&self, n: i64, x: T<'h>, scale: &Rat) -> T<'h> {
        let mut terms = vec![];
        let mut c = scale.clone();
        for k in 0..n {
            terms.push(self.ar.mul(&[self.q(&c), self.ar.pow(x, self.n(k))]));
            c = c.mul(&Rat::new(Int::from_i64(1), Int::from_i64(k + 1)));
        }
        self.ar.mul(&[self.exp_neg(x), self.ar.add(&terms)])
    }
    fn pos_int(&self, t: T<'h>, max: i64) -> Option<i64> {
        let q = self.rat(t)?;
        let n = if q.is_int() { q.num.to_i64()? } else { return None };
        (1..=max).contains(&n).then_some(n)
    }
    fn positive(&self, t: T<'h>) -> Option<bool> {
        Some(self.rat(t)?.cmp_value(&Rat::from_i64(0)).is_gt())
    }
    /// x^(-1/2) sqrt(2/pi) = (pi x/2)^(-1/2)
    fn half_order_pre(&self, x: T<'h>) -> T<'h> {
        let arg = self.ar.mul(&[self.q(&half()), self.sym("pi"), x]);
        self.ar.pow(arg, self.q(&half().neg()))
    }
    fn third(&self, num: i64, den: i64) -> T<'h> {
        self.q(&Rat::new(Int::from_i64(num), Int::from_i64(den)))
    }

    /// The rule for `f` at `args` (exact numbers or symbolic terms), if any.
    pub fn rule(&self, f: Special, a: &[T<'h>]) -> Option<Exact<'h>> {
        let pi = || self.sym("pi");
        match f {
            S::Erf | S::Erfi | S::SinIntegral | S::SinhIntegral | S::InverseErf => {
                if self.is(a[0], 0) {
                    return self.val(self.n(0));
                }
                if f == S::InverseErf && (self.is(a[0], 1) || self.is(a[0], -1)) {
                    return Some(Exact::Pole);
                }
                None
            }
            S::Erfc => self.is(a[0], 0).then(|| Exact::Value(self.n(1))),
            S::InverseErfc => {
                if self.is(a[0], 1) {
                    self.val(self.n(0))
                } else if self.is(a[0], 0) || self.is(a[0], 2) {
                    Some(Exact::Pole)
                } else {
                    None
                }
            }
            S::GammaInc | S::GammaRegularized => {
                let reg = f == S::GammaRegularized;
                if self.is(a[1], 0) {
                    return match self.positive(a[0])? {
                        true if reg => self.val(self.n(1)),
                        true => self.val(self.call("gamma", &[a[0]])),
                        false if !reg && self.rat(a[0])?.is_int() => Some(Exact::Pole),
                        false => None,
                    };
                }
                if let Some(n) = self.pos_int(a[0], 20) {
                    let mut scale = Rat::from_i64(1);
                    if !reg {
                        for k in 1..n {
                            scale = scale.mul(&Rat::from_i64(k));
                        }
                    }
                    return self.val(self.exp_poly(n, a[1], &scale));
                }
                if !reg && self.rat(a[0]).is_some_and(|q| q.cmp_value(&half()).is_eq()) {
                    let e = self.call("erfc", &[self.ar.pow(a[1], self.q(&half()))]);
                    return self.val(self.ar.mul(&[self.ar.pow(pi(), self.q(&half())), e]));
                }
                None
            }
            S::BetaInc | S::BetaRegularized => {
                if self.is(a[0], 0) {
                    return self.val(self.n(0));
                }
                if self.is(a[0], 1) {
                    return if f == S::BetaRegularized {
                        self.val(self.n(1))
                    } else {
                        self.val(self.call("beta", &[a[1], a[2]]))
                    };
                }
                None
            }
            S::ExpIntegralEi | S::CosIntegral | S::CoshIntegral => self.is(a[0], 0).then_some(Exact::Pole),
            S::LogIntegral => {
                if self.is(a[0], 0) {
                    self.val(self.n(0))
                } else if self.is(a[0], 1) {
                    Some(Exact::Pole)
                } else {
                    None
                }
            }
            S::ExpIntegralE => {
                if self.is(a[0], 0) && !self.is(a[1], 0) {
                    return self.val(self.ar.div(self.exp_neg(a[1]), a[1]));
                }
                if self.is(a[1], 0) {
                    let nu = self.rat(a[0])?;
                    return if nu.cmp_value(&Rat::from_i64(1)).is_gt() {
                        self.val(self.q(&nu.sub(&Rat::from_i64(1)).recip()?))
                    } else {
                        Some(Exact::Pole)
                    };
                }
                None
            }
            S::Polylog => {
                let z = a[1];
                if self.is(z, 0) {
                    return self.val(self.n(0));
                }
                let one_minus = || self.ar.sub(self.n(1), z);
                if self.is(a[0], 1) {
                    if self.is(z, 1) {
                        return Some(Exact::Pole);
                    }
                    return self.val(self.ar.neg(self.call("log", &[one_minus()])));
                }
                if self.is(a[0], 0) && !self.is(z, 1) {
                    return self.val(self.ar.div(z, one_minus()));
                }
                if self.is(a[0], -1) && !self.is(z, 1) {
                    return self.val(self.ar.div(z, self.ar.pow(one_minus(), self.n(2))));
                }
                let s = self.rat(a[0])?;
                if self.is(z, 1) {
                    return if s.cmp_value(&Rat::from_i64(1)).is_gt() {
                        self.val(self.call("zeta", &[a[0]]))
                    } else {
                        Some(Exact::Pole)
                    };
                }
                if self.is(z, -1) && s.is_int() {
                    // (2^(1-s) - 1) zeta(s)
                    let c = self.ar.sub(self.ar.pow(self.n(2), self.ar.sub(self.n(1), a[0])), self.n(1));
                    return self.val(self.ar.mul(&[c, self.call("zeta", &[a[0]])]));
                }
                if self.is(a[0], 2) && self.rat(z).is_some_and(|q| q.cmp_value(&half()).is_eq()) {
                    let p2 = self.ar.mul(&[self.third(1, 12), self.ar.pow(pi(), self.n(2))]);
                    let l2 = self.ar.mul(&[self.third(-1, 2), self.ar.pow(self.call("log", &[self.n(2)]), self.n(2))]);
                    return self.val(self.ar.add(&[p2, l2]));
                }
                None
            }
            S::Hyp0F1 => self.is(a[1], 0).then(|| Exact::Value(self.n(1))),
            S::Hyp1F1 => {
                if self.is(a[2], 0) {
                    return self.val(self.n(1));
                }
                if a[0] == a[1] {
                    return self.val(self.call("exp", &[a[2]]));
                }
                None
            }
            S::Hyp2F1 => {
                if self.is(a[3], 0) || self.is(a[0], 0) || self.is(a[1], 0) {
                    return self.val(self.n(1));
                }
                if self.is(a[3], 1) {
                    let (x, y, c) = (self.rat(a[0])?, self.rat(a[1])?, self.rat(a[2])?);
                    let d = c.sub(&x).sub(&y);
                    if d.cmp_value(&Rat::from_i64(0)).is_gt() {
                        let g = |q: &Rat| self.call("gamma", &[self.q(q)]);
                        let num = self.ar.mul(&[g(&c), g(&d)]);
                        let den = self.ar.mul(&[g(&c.sub(&x)), g(&c.sub(&y))]);
                        return self.val(self.ar.div(num, den));
                    }
                }
                None
            }
            S::BesselJ | S::BesselY => {
                let nu = self.rat(a[0])?;
                let x = a[1];
                if self.is(x, 0) {
                    if f == S::BesselY {
                        return nu.is_int().then_some(Exact::Pole);
                    }
                    if nu.is_zero() {
                        return self.val(self.n(1));
                    }
                    if !nu.num.neg || nu.is_int() {
                        return self.val(self.n(0));
                    }
                    return Some(Exact::Pole);
                }
                if self.ar.num(x).is_some_and(|v| !v.is_exact() || v.as_real().is_some_and(|r| r.is_negative())) {
                    return None;
                }
                let h = half();
                let (fun, neg) = match f {
                    S::BesselJ if nu.cmp_value(&h).is_eq() => ("sin", false),
                    S::BesselJ if nu.cmp_value(&h.neg()).is_eq() => ("cos", false),
                    S::BesselY if nu.cmp_value(&h).is_eq() => ("cos", true),
                    S::BesselY if nu.cmp_value(&h.neg()).is_eq() => ("sin", false),
                    _ => return None,
                };
                let v = self.ar.mul(&[self.half_order_pre(x), self.call(fun, &[x])]);
                self.val(if neg { self.ar.neg(v) } else { v })
            }
            S::AiryAi | S::AiryBi | S::AiryAiPrime | S::AiryBiPrime => {
                if !self.is(a[0], 0) {
                    return None;
                }
                // Ai(0) = 3^(-2/3)/gamma(2/3), Bi(0) = 3^(-1/6)/gamma(2/3), Ai'(0) = -3^(-1/3)/gamma(1/3),
                // Bi'(0) = 3^(1/6)/gamma(1/3)
                let (e, g, neg) = match f {
                    S::AiryAi => ((-2, 3), (2, 3), false),
                    S::AiryBi => ((-1, 6), (2, 3), false),
                    S::AiryAiPrime => ((-1, 3), (1, 3), true),
                    _ => ((1, 6), (1, 3), false),
                };
                let num = self.ar.pow(self.n(3), self.third(e.0, e.1));
                let v = self.ar.div(num, self.call("gamma", &[self.third(g.0, g.1)]));
                self.val(if neg { self.ar.neg(v) } else { v })
            }
            S::EllipticK | S::EllipticE => {
                if self.is(a[0], 0) {
                    return self.val(self.ar.mul(&[self.q(&half()), pi()]));
                }
                if self.is(a[0], 1) {
                    return if f == S::EllipticK { Some(Exact::Pole) } else { self.val(self.n(1)) };
                }
                None
            }
            S::EllipticF | S::EllipticEInc => {
                if self.is(a[0], 0) {
                    return self.val(self.n(0));
                }
                if self.is(a[1], 0) {
                    return self.val(a[0]);
                }
                None
            }
            S::EllipticPi => self.is(a[0], 0).then(|| Exact::Value(self.call("elliptic_k", &[a[1]]))),
            S::EllipticPiInc => self.is(a[0], 0).then(|| Exact::Value(self.call("elliptic_f", &[a[1], a[2]]))),
            S::JacobiSn | S::JacobiCn | S::JacobiDn => {
                let (u, m) = (a[0], a[1]);
                if self.is(u, 0) {
                    return self.val(self.n(if f == S::JacobiSn { 0 } else { 1 }));
                }
                if self.is(m, 0) {
                    return self.val(match f {
                        S::JacobiSn => self.call("sin", &[u]),
                        S::JacobiCn => self.call("cos", &[u]),
                        _ => self.n(1),
                    });
                }
                if self.is(m, 1) {
                    return self.val(match f {
                        S::JacobiSn => self.call("tanh", &[u]),
                        _ => self.ar.pow(self.call("cosh", &[u]), self.n(-1)),
                    });
                }
                None
            }
            S::InverseJacobiSn => self.is(a[0], 0).then(|| Exact::Value(self.n(0))),
            _ => None,
        }
    }
}
