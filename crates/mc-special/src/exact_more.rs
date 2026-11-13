//! Exact values of the item-3C functions (decisions SF.1, SF.2, 3C.2): closed forms at exact special arguments,
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

/// The exact rules of the item-3C functions.
pub struct More<'a, 'h> {
    h: &'h Heap,
    ar: &'a Arith<'h, Heap>,
}

impl<'a, 'h> More<'a, 'h> {
    /// Is `t` exactly pi/2?
    fn is_half_pi(&self, t: T<'h>) -> bool {
        let want = self.ar.mul(&[self.ar.rational(self.h.int(1), self.h.int(2)), self.h.sym_term(self.h.sym("pi"))]);
        t == want
    }
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
    /// `x` when `t` is `-x` written with a negative sign: a negative exact number, or a product with a negative
    /// exact coefficient.
    fn negated(&self, t: T<'h>) -> Option<T<'h>> {
        let neg = |u: T<'h>| self.rat(u).is_some_and(|q| q.cmp_value(&Rat::from_i64(0)).is_lt());
        if neg(t) {
            return Some(self.ar.neg(t));
        }
        match self.h.view(t) {
            mc_core::View::App { head, args } if matches!(self.h.view(head), mc_core::View::Sym(s) if self.h.sym_name(s) == "mul") && args.len() >= 2 && neg(args[0]) => {
                Some(self.ar.neg(t))
            }
            _ => None,
        }
    }
    /// A square root of `y` for a rule that is even in it (cosh, or sinh(s)/s): c u^2 gives sqrt(c) u whatever the
    /// sign of u, and for c < 0 the root sqrt(-c) u with `true`: the rule is then the circular one (cosh(im t) =
    /// cos t); `None` when `y` is not of that form.
    fn even_sqrt(&self, y: T<'h>) -> Option<(T<'h>, bool)> {
        let is_sym = |t: T<'h>, name: &str| matches!(self.h.view(t), mc_core::View::Sym(s) if self.h.sym_name(s) == name);
        let app = |t: T<'h>, name: &str| match self.h.view(t) {
            mc_core::View::App { head, args } if is_sym(head, name) => Some(args),
            _ => None,
        };
        let (coeff, rest): (Rat, Vec<T<'h>>) = match app(y, "mul") {
            Some(args) => match args.first().and_then(|&c| self.rat(c)) {
                Some(c) => (c, args[1..].to_vec()),
                None => (Rat::from_i64(1), args.to_vec()),
            },
            None => (Rat::from_i64(1), vec![y]),
        };
        let square = match rest.as_slice() {
            [u] => app(*u, "pow").and_then(|a| (a.len() == 2 && self.is(a[1], 2)).then_some(a[0])),
            _ => None,
        };
        let half = self.third(1, 2);
        match square {
            Some(u) if !coeff.is_zero() => {
                let neg = coeff.cmp_value(&Rat::from_i64(0)).is_lt();
                let c = if neg { coeff.neg() } else { coeff };
                Some((self.ar.mul(&[self.ar.pow(self.q(&c), half), u]), neg))
            }
            _ => None,
        }
    }
    fn val(&self, t: T<'h>) -> Option<Exact<'h>> {
        Some(Exact::Value(t))
    }
    /// A terminating series: an upper parameter an exact integer -n (0 < n <= 40) and some parameter or the
    /// argument symbolic, so the value is the polynomial sum_{k<=n} prod (a_i)_k / prod (b_j)_k z^k / k!. `None`
    /// when a lower parameter is an exact integer in (-n, 0] (the series meets a pole first) or all is numeric (the
    /// numeric paths decide).
    fn terminating(&self, up: &[T<'h>], low: &[T<'h>], z: T<'h>) -> Option<Exact<'h>> {
        let n = up.iter().filter_map(|&a| self.rat(a)).filter(|q| q.is_int() && q.num.neg && !q.is_zero()).filter_map(|q| q.num.to_i64()).map(|v| -v).min()?;
        if n > 40 {
            return None;
        }
        // numbers only: exact rationals give the exact value (2F1(3/2, -4; 1/2; 2) = 17), anything else is numeric work
        let all = up.iter().chain(low).chain([&z]);
        if all.clone().all(|&t| self.ar.num(t).is_some()) && !all.clone().all(|&t| self.rat(t).is_some()) {
            return None;
        }
        for &b in low {
            if let Some(q) = self.rat(b) {
                if q.is_int() && q.cmp_value(&Rat::from_i64(0)).is_le() && q.cmp_value(&Rat::from_i64(-n)).is_gt() {
                    return None;
                }
            }
        }
        let mut term = self.n(1);
        let mut parts = vec![term];
        for k in 0..n {
            let kk = self.n(k);
            let mut num: Vec<T<'h>> = up.iter().map(|&a| self.ar.add(&[a, kk])).collect();
            num.push(z);
            let mut den: Vec<T<'h>> = low.iter().map(|&b| self.ar.add(&[b, kk])).collect();
            den.push(self.n(k + 1));
            term = self.ar.div(self.ar.mul(&[term, self.ar.mul(&num)]), self.ar.mul(&den));
            parts.push(term);
        }
        self.val(self.ar.add(&parts))
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
        // indeterminate arguments, and the gamma family at complex_infinity, give indeterminate
        if a.iter().any(|&t| t == self.sym("indeterminate"))
            || (matches!(f, S::Gamma | S::LogGamma | S::Digamma | S::Polygamma) && a.last() == Some(&self.sym("complex_infinity")))
        {
            return self.val(self.sym("indeterminate"));
        }
        match f {
            S::Erf | S::Erfi | S::SinIntegral | S::SinhIntegral | S::InverseErf => {
                if self.is(a[0], 0) {
                    return self.val(self.n(0));
                }
                if f == S::InverseErf && (self.is(a[0], 1) || self.is(a[0], -1)) {
                    return Some(Exact::Pole);
                }
                // odd functions: f(-x) = -f(x) for an argument with a negative sign
                if let Some(m) = self.negated(a[0]) {
                    return self.val(self.ar.neg(self.call(f.name(), &[m])));
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
                // I_0(0, b): the limit in z depends on the order of the limits in z and a (B(z; 0, b) ~ log z)
                if self.is(a[0], 0) && self.is(a[1], 0) && f == S::BetaRegularized {
                    return self.val(self.sym("indeterminate"));
                }
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
                // Li_-m(z) = z A_m(z)/(1 - z)^(m+1) with the Eulerian polynomial A_m (m = 2..60), for any z
                if let Some(m) = s.is_int().then(|| s.num.to_i64()).flatten().filter(|n| (-60..=-2).contains(n)) {
                    let m = (-m) as usize;
                    let mut row: Vec<Int> = vec![Int::from_i64(1)];
                    for r in 2..=m {
                        let mut next = Vec::with_capacity(r);
                        for k in 0..r {
                            let mut v = Int::from_i64(0);
                            if k < r - 1 {
                                v = v.add(&row[k].mul(&Int::from_i64(k as i64 + 1)));
                            }
                            if k > 0 {
                                v = v.add(&row[k - 1].mul(&Int::from_i64((r - k) as i64)));
                            }
                            next.push(v);
                        }
                        row = next;
                    }
                    let terms: Vec<T<'h>> = row
                        .iter()
                        .enumerate()
                        .map(|(k, c)| self.ar.mul(&[self.q(&Rat::new(c.clone(), Int::from_i64(1))), self.ar.pow(z, self.n(k as i64))]))
                        .collect();
                    let num = self.ar.mul(&[z, self.ar.add(&terms)]);
                    return self.val(self.ar.div(num, self.ar.pow(one_minus(), self.n(m as i64 + 1))));
                }
                if self.is(a[0], 2) && self.rat(z).is_some_and(|q| q.cmp_value(&half()).is_eq()) {
                    let p2 = self.ar.mul(&[self.third(1, 12), self.ar.pow(pi(), self.n(2))]);
                    let l2 = self.ar.mul(&[self.third(-1, 2), self.ar.pow(self.call("log", &[self.n(2)]), self.n(2))]);
                    return self.val(self.ar.add(&[p2, l2]));
                }
                None
            }
            S::Hyp0F1 => {
                if self.is(a[1], 0) {
                    return self.val(self.n(1));
                }
                // symbolic argument, b = 1/2 or 3/2: 0F1(; 1/2; w) = cosh(2 sqrt w), 0F1(; 3/2; w) = sinh(2 sqrt w)/(2
                // sqrt w) (DLMF 10.16.1 with 10.39.1-2), even in sqrt w, so no branch enters
                // (only for a argument of the form c u^2: for a general w the principal sqrt(w) would make later
                // real-domain analyses of the result, a convexity test say, read the function as real)
                if self.ar.num(a[1]).is_none() {
                    let q = self.rat(a[0])?;
                    let (r, circ) = self.even_sqrt(a[1])?;
                    let s = self.ar.mul(&[self.n(2), r]);
                    if q.cmp_value(&half()).is_eq() {
                        return self.val(self.call(if circ { "cos" } else { "cosh" }, &[s]));
                    }
                    if q.cmp_value(&Rat::new(Int::from_i64(3), Int::from_i64(2))).is_eq() {
                        return self.val(self.ar.div(self.call(if circ { "sin" } else { "sinh" }, &[s]), s));
                    }
                }
                None
            }
            S::Hyp1F1 => {
                if self.is(a[2], 0) {
                    return self.val(self.n(1));
                }
                // 1F1(0; b; z) = 1 for every b (also b = 0, where a = b is 0/0: the series has no later terms)
                if self.is(a[0], 0) {
                    return self.val(self.n(1));
                }
                if a[0] == a[1] {
                    return self.val(self.call("exp", &[a[2]]));
                }
                self.terminating(&[a[0]], &[a[1]], a[2])
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
                // 2F1(a, b; b; z) = (1 - z)^-a (DLMF 15.4.6), 2F1(1, 1; 2; z) = -log(1 - z)/z (15.4.1),
                // 2F1(1/2, 1; 3/2; z^2) is not here (needs a square root of z)
                let omz = self.ar.sub(self.n(1), a[3]);
                if !self.is(a[3], 1) && a[1] == a[2] && self.rat(a[2]).is_none_or(|q| !((q.is_int() && q.num.neg) || q.is_zero())) {
                    return self.val(self.ar.pow(omz, self.ar.neg(a[0])));
                }
                if !self.is(a[3], 1) && a[0] == a[2] && self.rat(a[2]).is_none_or(|q| !((q.is_int() && q.num.neg) || q.is_zero())) {
                    return self.val(self.ar.pow(omz, self.ar.neg(a[1])));
                }
                if self.is(a[0], 1) && self.is(a[1], 1) && self.is(a[2], 2) && !self.is(a[3], 1) {
                    return self.val(self.ar.neg(self.ar.div(self.call("log", &[omz]), a[3])));
                }
                // Euler's transformation (DLMF 15.8.1) when c - a or c - b is a non-positive integer: the second factor
                // terminates, 2F1(3/2, 5; 1/2; z) = (1 - z)^-6 2F1(-1, -9/2; 1/2; z)
                if let (Some(x), Some(y), Some(c)) = (self.rat(a[0]), self.rat(a[1]), self.rat(a[2])) {
                    let is_term = |q: &Rat| q.is_int() && (q.is_zero() || q.num.neg);
                    let (ca, cb) = (c.sub(&x), c.sub(&y));
                    if (is_term(&ca) || is_term(&cb)) && !is_term(&x) && !is_term(&y) && !is_term(&c) {
                        let e = c.sub(&x).sub(&y);
                        if let Some(Exact::Value(v)) = self.terminating(&[self.q(&ca), self.q(&cb)], &[a[2]], a[3]) {
                            return self.val(self.ar.mul(&[self.ar.pow(omz, self.q(&e)), v]));
                        }
                    }
                }
                if let Some(t) = self.terminating(&[a[0], a[1]], &[a[2]], a[3]) {
                    return Some(t);
                }
                // parameters differing by integers: elementary functions and logarithms (crate::rathyp)
                let (x, y, c) = (self.rat(a[0])?, self.rat(a[1])?, self.rat(a[2])?);
                crate::rathyp::closed_form(self.h, self.ar, &[x, y], &[c], a[3]).map(Exact::Value)
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
                // F(pi/2 | m) = K(m), E(pi/2 | m) = E(m) (DLMF 19.2.8, 19.2.5)
                if self.is_half_pi(a[0]) {
                    return self.val(self.call(if f == S::EllipticF { "elliptic_k" } else { "elliptic_e" }, &[a[1]]));
                }
                None
            }
            S::EllipticPi => {
                if self.is(a[0], 0) {
                    return self.val(self.call("elliptic_k", &[a[1]]));
                }
                // Pi(n | 0) = pi/(2 sqrt(1 - n)) (DLMF 19.6.1)
                if self.is(a[1], 0) {
                    let one = self.n(1);
                    let root = self.ar.pow(self.ar.sub(one, a[0]), self.ar.rational(self.h.int(1), self.h.int(2)));
                    let pi = self.h.sym_term(self.h.sym("pi"));
                    return self.val(self.ar.div(pi, self.ar.mul(&[self.n(2), root])));
                }
                None
            }
            S::EllipticPiInc => {
                if self.is(a[0], 0) {
                    return self.val(self.call("elliptic_f", &[a[1], a[2]]));
                }
                // Pi(n; 0 | m) = 0, Pi(n; pi/2 | m) = Pi(n | m) (DLMF 19.2.7)
                if self.is(a[1], 0) {
                    return self.val(self.n(0));
                }
                if self.is_half_pi(a[1]) {
                    return self.val(self.call("elliptic_pi", &[a[0], a[2]]));
                }
                None
            }
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
                        _ => self.call("sech", &[u]),
                    });
                }
                // x + a K(m) + b i K(1 - m): the value at the quarter periods, or a constant times a function of x
                let p = match f {
                    S::JacobiSn => 's',
                    S::JacobiCn => 'c',
                    _ => 'd',
                };
                crate::jshift::reduce(self.h, self.ar, p, 'n', u, m).map(Exact::Value)
            }
            S::InverseJacobiSn => self.is(a[0], 0).then(|| Exact::Value(self.n(0))),
            _ => None,
        }
    }
}
