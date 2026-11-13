//! Exact identities of the elementary functions (semantics 15): special values at rational multiples of pi, the
//! inverse functions at those values, parity, and compositions that hold for every complex argument. Each returns
//! the replacement term (evaluated again by the caller) or `None`; an argument without an identity stays as the
//! call (decision L.2).
use crate::consts::Const;
use crate::fun::Fun;
use crate::numeval::Names;
use mc_api::{Store, View, T};
use mc_arith::num::{Int, Rat};
use mc_arith::{Arith, Num, Real};

/// The identities for one store.
pub struct Exact<'a, 'h, S: Store + ?Sized> {
    st: &'h S,
    ar: &'a Arith<'h, S>,
    nm: &'a Names,
}

fn q(n: i64, d: i64) -> Rat {
    Rat::new(Int::from_i64(n), Int::from_i64(d))
}

impl<'a, 'h, S: Store + ?Sized> Exact<'a, 'h, S> {
    /// The identities for terms of `st`.
    pub fn new(st: &'h S, ar: &'a Arith<'h, S>, nm: &'a Names) -> Self {
        Exact { st, ar, nm }
    }

    fn rat_term(&self, r: &Rat) -> T<'h> {
        self.ar.number(&Num::Real(Real::Exact(r.clone())))
    }

    fn rat(&self, t: T<'h>) -> Option<Rat> {
        match self.ar.num(t)? {
            Num::Real(Real::Exact(r)) => Some(r),
            _ => None,
        }
    }

    fn constant(&self, c: Const) -> T<'h> {
        self.st.sym_term(self.nm.sym_of(c))
    }

    fn call(&self, f: Fun, x: T<'h>) -> T<'h> {
        self.st.call(self.nm.fun_sym(f), &[x])
    }

    /// q pi, canonical.
    fn times_pi(&self, r: &Rat) -> T<'h> {
        self.ar.mul(&[self.rat_term(r), self.constant(Const::Pi)])
    }

    /// The arguments of `mul(c, k)` with `k` the term `k_term` and c a number; the number.
    fn coefficient_of(&self, t: T<'h>, k_term: T<'h>) -> Option<Num> {
        if t == k_term {
            return Some(Num::int(1));
        }
        let View::App { head, args: [a, b] } = self.st.view(t) else {
            return None;
        };
        if head != self.st.sym_term(self.nm.mul()) {
            return None;
        }
        match (self.ar.num(*a), self.ar.num(*b)) {
            (Some(c), None) if *b == k_term => Some(c),
            (None, Some(c)) if *a == k_term => Some(c),
            _ => None,
        }
    }

    /// q with t = q pi (q rational, t = 0 included); also t = k degree (q = k/180).
    fn pi_multiple(&self, t: T<'h>) -> Option<Rat> {
        if let Some(r) = self.rat(t) {
            return r.is_zero().then_some(r);
        }
        if let Some(Num::Real(Real::Exact(r))) = self.coefficient_of(t, self.constant(Const::Pi)) {
            return Some(r);
        }
        match self.coefficient_of(t, self.constant(Const::Degree))? {
            Num::Real(Real::Exact(r)) => Some(r.mul(&q(1, 180))),
            _ => None,
        }
    }

    /// b with t = b i pi or t = b i (b real exact), for the hyperbolic functions of imaginary arguments: the
    /// imaginary coefficient and whether pi is a factor.
    fn imaginary_part_of(&self, t: T<'h>) -> Option<(Rat, bool)> {
        if let Some(Num::Complex(re, Real::Exact(b))) = self.ar.num(t) {
            return re.is_exact_zero().then_some((b, false));
        }
        match self.coefficient_of(t, self.constant(Const::Pi))? {
            Num::Complex(re, Real::Exact(b)) if re.is_exact_zero() => Some((b, true)),
            _ => None,
        }
    }

    /// -t when t has a negative exact coefficient (a negative rational, or a product with one).
    fn negated(&self, t: T<'h>) -> Option<T<'h>> {
        let negative = match self.st.view(t) {
            View::App { head, args } if head == self.st.sym_term(self.nm.mul()) => {
                args.iter().any(|&a| self.rat(a).is_some_and(|r| r.num.neg))
            }
            _ => self.rat(t).is_some_and(|r| r.num.neg),
        };
        negative.then(|| self.ar.neg(t))
    }

    fn sqrt(&self, t: T<'h>) -> T<'h> {
        self.ar.pow(t, self.rat_term(&q(1, 2)))
    }

    fn int(&self, v: i64) -> T<'h> {
        self.st.int(v)
    }

    /// sin(x pi) for x in [0, 1/2] at the angles with radical values (denominators 1, 2, 3, 4, 5, 6, 10, 12), in
    /// the forms of the usual tables.
    fn sin_table(&self, x: &Rat) -> Option<T<'h>> {
        let r = |n, d| self.rat_term(&q(n, d));
        let s5 = || self.sqrt(self.int(5));
        let s3 = || self.sqrt(self.int(3));
        let over_2s2 = |t: T<'h>| self.ar.mul(&[r(1, 2), self.ar.pow(self.int(2), r(-1, 2)), t]);
        Some(if x.is_zero() {
            self.int(0)
        } else if *x == q(1, 12) {
            over_2s2(self.ar.add(&[s3(), self.int(-1)]))
        } else if *x == q(1, 10) {
            self.ar.mul(&[r(1, 4), self.ar.add(&[s5(), self.int(-1)])])
        } else if *x == q(1, 6) {
            r(1, 2)
        } else if *x == q(1, 5) {
            self.sqrt(self.ar.add(&[r(5, 8), self.ar.mul(&[r(-1, 8), s5()])]))
        } else if *x == q(1, 4) {
            self.ar.mul(&[r(1, 2), self.sqrt(self.int(2))])
        } else if *x == q(3, 10) {
            self.ar.mul(&[r(1, 4), self.ar.add(&[s5(), self.int(1)])])
        } else if *x == q(1, 3) {
            self.ar.mul(&[r(1, 2), s3()])
        } else if *x == q(2, 5) {
            self.sqrt(self.ar.add(&[r(5, 8), self.ar.mul(&[r(1, 8), s5()])]))
        } else if *x == q(5, 12) {
            over_2s2(self.ar.add(&[s3(), self.int(1)]))
        } else if *x == q(1, 2) {
            self.int(1)
        } else {
            return None;
        })
    }

    /// tan(x pi) for x in [0, 1/2) at the table's angles.
    fn tan_table(&self, x: &Rat) -> Option<T<'h>> {
        let r = |n, d| self.rat_term(&q(n, d));
        let s5 = || self.sqrt(self.int(5));
        let s3 = || self.sqrt(self.int(3));
        Some(if x.is_zero() {
            self.int(0)
        } else if *x == q(1, 12) {
            self.ar.add(&[self.int(2), self.ar.neg(s3())])
        } else if *x == q(1, 10) {
            self.sqrt(self.ar.add(&[self.int(1), self.ar.mul(&[self.int(-2), self.ar.pow(self.int(5), r(-1, 2))])]))
        } else if *x == q(1, 6) {
            self.ar.pow(self.int(3), r(-1, 2))
        } else if *x == q(1, 5) {
            self.sqrt(self.ar.add(&[self.int(5), self.ar.mul(&[self.int(-2), s5()])]))
        } else if *x == q(1, 4) {
            self.int(1)
        } else if *x == q(3, 10) {
            self.sqrt(self.ar.add(&[self.int(1), self.ar.mul(&[self.int(2), self.ar.pow(self.int(5), r(-1, 2))])]))
        } else if *x == q(1, 3) {
            s3()
        } else if *x == q(2, 5) {
            self.sqrt(self.ar.add(&[self.int(5), self.ar.mul(&[self.int(2), s5()])]))
        } else if *x == q(5, 12) {
            self.ar.add(&[self.int(2), s3()])
        } else {
            return None;
        })
    }

    /// sin(q pi) where it is a rational or a radical: q reduced modulo 2 to [0, 1/2] by symmetry.
    fn sin_pi(&self, r: &Rat) -> Option<T<'h>> {
        let (_, frac) = r.mul(&q(1, 2)).floor_fract();
        let mut x = frac.mul(&Rat::from_i64(2));
        let mut neg = false;
        if x.cmp_value(&Rat::from_i64(1)).is_ge() {
            x = x.sub(&Rat::from_i64(1));
            neg = true;
        }
        if x.cmp_value(&q(1, 2)).is_gt() {
            x = Rat::from_i64(1).sub(&x);
        }
        let v = self.sin_table(&x)?;
        Some(if neg { self.ar.neg(v) } else { v })
    }

    fn cos_pi(&self, r: &Rat) -> Option<T<'h>> {
        self.sin_pi(&r.add(&q(1, 2)))
    }

    /// tan(q pi): q reduced modulo 1 to (-1/2, 1/2]; `None` at the poles.
    fn tan_pi(&self, r: &Rat) -> Option<T<'h>> {
        let (_, mut x) = r.floor_fract();
        if x.cmp_value(&q(1, 2)).is_gt() {
            x = x.sub(&Rat::from_i64(1));
        }
        if x == q(1, 2) {
            return None;
        }
        let neg = x.num.neg;
        let v = self.tan_table(&if neg { x.neg() } else { x })?;
        Some(if neg { self.ar.neg(v) } else { v })
    }

    /// r pi with f(r pi) = x, for the table of f's special values on [0, 1/2]; `inverse(sin, x)` and so on.
    fn inverse(&self, value: impl Fn(&Rat) -> Option<T<'h>>, x: T<'h>) -> Option<Rat> {
        Self::classic_angles().into_iter().find(|r| value(r) == Some(x))
    }

    /// The angles whose values the inverse functions recognise, and where exp(q pi i) expands (denominators 1, 2,
    /// 3, 4, 6).
    fn classic_angles() -> [Rat; 5] {
        [q(0, 1), q(1, 6), q(1, 4), q(1, 3), q(1, 2)]
    }

    /// Is q (reduced modulo 1/2) one of the classic angles?
    fn classic(r: &Rat) -> bool {
        let (_, frac) = r.mul(&Rat::from_i64(2)).floor_fract();
        let x = frac.mul(&q(1, 2));
        Self::classic_angles().iter().any(|a| *a == x || q(1, 2).sub(a) == x)
    }

    /// The identity for f(x), if one applies.
    pub fn simplify(&self, f: Fun, x: T<'h>) -> Option<T<'h>> {
        // parity: f(-x) = -f(x) or f(x)
        if let (Some(odd), Some(y)) = (f.parity(), self.negated(x)) {
            let fy = self.call(f, y);
            return Some(if odd { self.ar.neg(fy) } else { fy });
        }
        let zero = self.st.int(0);
        let one = self.st.int(1);
        if let Some(v) = self.trig_of_angle(f, x) {
            return Some(v);
        }
        // f(g(y)) = y for g the inverse of f
        if let View::App { head, args: [y] } = self.st.view(x) {
            let inner = match self.st.view(head) {
                View::Sym(s) => self.nm.fun(s),
                _ => None,
            };
            let inverse = matches!(
                (f, inner),
                (Fun::Exp, Some(Fun::Log))
                    | (Fun::Sin, Some(Fun::Asin))
                    | (Fun::Cos, Some(Fun::Acos))
                    | (Fun::Tan, Some(Fun::Atan))
            );
            if inverse {
                return Some(*y);
            }
            // sin, cos, tan of asin, acos, atan: algebraic in y for every complex y (principal square roots; the
            // cosine of asin y and atan y, the sine of acos y have a nonnegative real part)
            if let Some(v) = inner.and_then(|g| self.trig_of_inverse(f, g, *y)) {
                return Some(v);
            }
            // log(exp(y)) = y for real y
            if f == Fun::Log && inner == Some(Fun::Exp) && matches!(self.ar.num(*y), Some(Num::Real(_))) {
                return Some(*y);
            }
        }
        if let Some(v) = self.at_infinity(f, x) {
            return Some(v);
        }
        if let Some(v) = self.imaginary_argument(f, x) {
            return Some(v);
        }
        match f {
            Fun::Sin => self.sin_pi(&self.pi_multiple(x)?),
            Fun::Cos => self.cos_pi(&self.pi_multiple(x)?),
            Fun::Tan => self.tan_pi(&self.pi_multiple(x)?),
            Fun::Asin => self.inverse(|r| self.sin_pi(r), x).map(|r| self.times_pi(&r)),
            Fun::Acos => {
                // acos(x) = pi/2 - asin(x); acos(-x) = pi - acos(x)
                if let Some(y) = self.negated(x) {
                    let r = self.inverse(|r| self.sin_pi(r), y)?;
                    return Some(self.times_pi(&q(1, 2).add(&r)));
                }
                let r = self.inverse(|r| self.sin_pi(r), x)?;
                Some(self.times_pi(&q(1, 2).sub(&r)))
            }
            Fun::Atan => Self::classic_angles()
                .into_iter()
                .find(|r| self.tan_pi(r) == Some(x))
                .map(|r| self.times_pi(&r)),
            Fun::Exp => self.exp(x),
            Fun::Log => self.log(x),
            Fun::Sinh | Fun::Tanh => (x == zero).then_some(zero),
            Fun::Cosh => (x == zero).then_some(one),
            Fun::Sqrt => None,
            Fun::Log1p | Fun::Expm1 => (x == zero).then_some(zero),
        }
    }

    /// cos, sin and tan of the angle `atan(a, b)` of the point (a, b): a/r, b/r and b/a with r = sqrt(a^2 + b^2).
    /// For real a, b not both zero these are the definitions of the polar angle (cos t = a/r, sin t = b/r); for
    /// complex a, b they hold for the angle -i log((a + i b)/r), whose exponential is (a + i b)/r and whose inverse
    /// exponential is (a - i b)/r since (a + i b)(a - i b) = r^2. Left alone where they do not hold: exact a, b with
    /// a^2 + b^2 = 0 (the origin, and (1, i), where r vanishes), tan with a = 0, and inexact arguments (the numeric
    /// evaluation).
    fn trig_of_angle(&self, f: Fun, x: T<'h>) -> Option<T<'h>> {
        if !matches!(f, Fun::Cos | Fun::Sin | Fun::Tan) {
            return None;
        }
        let View::App { head, args: [a, b] } = self.st.view(x) else { return None };
        let View::Sym(s) = self.st.view(head) else { return None };
        if self.nm.fun(s) != Some(Fun::Atan) {
            return None;
        }
        let (a, b) = (*a, *b);
        if [a, b].iter().any(|&t| self.ar.num(t).is_some_and(|n| !n.is_exact())) {
            return None;
        }
        let two = self.st.int(2);
        let r2 = self.ar.add(&[self.ar.pow(a, two), self.ar.pow(b, two)]);
        if self.ar.num(r2).is_some_and(|n| n.is_zero()) {
            return None;
        }
        let inv = |t: T<'h>| self.ar.pow(t, self.st.int(-1));
        let inv_r = || inv(self.sqrt(r2));
        Some(match f {
            Fun::Cos => self.ar.mul(&[a, inv_r()]),
            Fun::Sin => self.ar.mul(&[b, inv_r()]),
            _ => {
                if self.ar.num(a).is_some_and(|n| n.is_zero()) {
                    return None;
                }
                self.ar.mul(&[b, inv(a)])
            }
        })
    }

    /// f(g(y)) for f in sin, cos, tan and g in asin, acos, atan with f not the inverse of g, as an algebraic
    /// expression of y: cos(asin y) = sin(acos y) = sqrt(1 - y^2), cos(atan y) = 1/sqrt(1 + y^2), sin(atan y) =
    /// y/sqrt(1 + y^2), tan(asin y) = y/sqrt(1 - y^2), tan(acos y) = sqrt(1 - y^2)/y (Abramowitz and Stegun 4.4.x
    /// relations; they follow from sin^2 + cos^2 = 1 and the ranges of the principal branches). An inexact y is left
    /// to the numeric evaluation.
    fn trig_of_inverse(&self, f: Fun, g: Fun, y: T<'h>) -> Option<T<'h>> {
        if self.ar.num(y).is_some_and(|n| !n.is_exact()) {
            return None;
        }
        let one = self.st.int(1);
        let two = self.st.int(2);
        let y2 = self.ar.pow(y, two);
        // y = +-i (atan has its poles there) and y = +-1 for the quotients: left to the evaluator
        if self.ar.num(y2).is_some_and(|n| n.is_exact_minus_one() || n.is_exact_one()) {
            return None;
        }
        let one_minus = || self.sqrt(self.ar.add(&[one, self.ar.neg(y2)]));
        let one_plus = || self.sqrt(self.ar.add(&[one, y2]));
        let inv = |t: T<'h>| self.ar.pow(t, self.st.int(-1));
        Some(match (f, g) {
            (Fun::Cos, Fun::Asin) | (Fun::Sin, Fun::Acos) => one_minus(),
            (Fun::Cos, Fun::Atan) => inv(one_plus()),
            (Fun::Sin, Fun::Atan) => self.ar.mul(&[y, inv(one_plus())]),
            (Fun::Tan, Fun::Asin) => self.ar.mul(&[y, inv(one_minus())]),
            (Fun::Tan, Fun::Acos) => self.ar.mul(&[one_minus(), inv(y)]),
            _ => return None,
        })
    }

    fn exp(&self, x: T<'h>) -> Option<T<'h>> {
        if x == self.st.int(0) {
            return Some(self.st.int(1));
        }
        if x == self.st.int(1) {
            return Some(self.constant(Const::E));
        }
        if let Some(v) = self.exp_of_logs(x) {
            return Some(v);
        }
        // exp(q pi i) = cos(q pi) + i sin(q pi)
        let Num::Complex(re, Real::Exact(r)) = self.coefficient_of(x, self.constant(Const::Pi))? else {
            return None;
        };
        if !re.is_exact_zero() || !Self::classic(&r) {
            return None;
        }
        let (c, s) = (self.cos_pi(&r)?, self.sin_pi(&r)?);
        let i = self.ar.complex(self.st.int(0), self.st.int(1));
        Some(self.ar.add(&[c, self.ar.mul(&[i, s])]))
    }

    fn log(&self, x: T<'h>) -> Option<T<'h>> {
        let e = self.constant(Const::E);
        if x == self.st.int(1) {
            return Some(self.st.int(0));
        }
        if x == e {
            return Some(self.st.int(1));
        }
        if x == self.st.int(-1) {
            let i = self.ar.complex(self.st.int(0), self.st.int(1));
            return Some(self.ar.mul(&[i, self.constant(Const::Pi)]));
        }
        if x == self.st.int(0) {
            return Some(self.ar.neg(self.inf()));
        }
        // log(1/n) = -log(n) for an integer n > 1
        if let Some(Num::Real(Real::Exact(r))) = self.ar.num(x) {
            if !r.num.neg && !r.is_zero() {
                let inv = self.ar.div(self.st.int(1), x);
                if matches!(self.st.view(inv), View::Int(v) if v > 1) {
                    return Some(self.ar.neg(self.call(Fun::Log, inv)));
                }
            }
        }
        // log(r i^k) = log(r) + k i pi/2 for a positive rational r and an exact unit i^k other than 1
        if let Some(n) = self.ar.num(x) {
            let (re, im) = match n {
                Num::Real(Real::Exact(r)) => (r, Rat::from_i64(0)),
                Num::Complex(Real::Exact(a), Real::Exact(b)) => (a, b),
                _ => return None,
            };
            let (r, k) = if im.is_zero() && re.num.neg {
                (re.neg(), 2)
            } else if re.is_zero() && !im.num.neg {
                (im, 1)
            } else if re.is_zero() {
                (im.neg(), -1)
            } else {
                return None;
            };
            let angle = self.ar.mul(&[self.i(), self.rat_term(&q(k, 2)), self.constant(Const::Pi)]);
            let l = if r == Rat::from_i64(1) { self.st.int(0) } else { self.call(Fun::Log, self.rat_term(&r)) };
            return Some(self.ar.add(&[l, angle]));
        }
        // log(e^y) = y for real y
        let View::App { head, args: [b, y] } = self.st.view(x) else {
            return None;
        };
        (head == self.st.sym_term(self.nm.pow()) && *b == e && matches!(self.ar.num(*y), Some(Num::Real(_))))
            .then_some(*y)
    }

    fn inf(&self) -> T<'h> {
        self.st.sym_term(self.st.sym("inf"))
    }

    fn i(&self) -> T<'h> {
        self.ar.complex(self.st.int(0), self.st.int(1))
    }

    /// The direction of a directed infinity `inf`, `-inf`, `im*inf`, `-im*inf` in quarter turns: 0, 2, 1, -1.
    pub fn infinity_direction(&self, x: T<'h>) -> Option<i8> {
        let inf = self.inf();
        if x == inf {
            return Some(0);
        }
        let View::App { head, args: [c, y] } = self.st.view(x) else { return None };
        if head != self.st.sym_term(self.nm.mul()) || *y != inf {
            return None;
        }
        match self.ar.num(*c)? {
            Num::Real(Real::Exact(r)) if r == Rat::from_i64(-1) => Some(2),
            Num::Complex(re, Real::Exact(b)) if re.is_exact_zero() && b == Rat::from_i64(1) => Some(1),
            Num::Complex(re, Real::Exact(b)) if re.is_exact_zero() && b == Rat::from_i64(-1) => Some(-1),
            _ => None,
        }
    }

    /// The limits of the elementary functions at the directed infinities where they exist.
    fn at_infinity(&self, f: Fun, x: T<'h>) -> Option<T<'h>> {
        // |log z| -> inf as |z| -> inf in any direction, so log(complex_infinity) is inf (the real part dominates)
        if x == self.st.sym_term(self.st.sym("complex_infinity")) {
            return match f {
                Fun::Log => Some(self.inf()),
                // no limit as |z| -> inf: exp tends to 0 along -inf and to inf along inf; sin, cos, tan and their
                // hyperbolic forms oscillate along one axis and grow (or tend to +-i) along the other; atan tends to
                // pi/2 in the right half plane and to -pi/2 in the left
                Fun::Exp | Fun::Sin | Fun::Cos | Fun::Tan | Fun::Sinh | Fun::Cosh | Fun::Tanh | Fun::Atan | Fun::Expm1 => {
                    Some(self.st.sym_term(self.st.sym("indeterminate")))
                }
                _ => None,
            };
        }
        let d = self.infinity_direction(x)?;
        let inf = self.inf();
        let half_pi = self.times_pi(&q(1, 2));
        // a periodic function at a real infinity has no limit: its values accumulate on its whole range, given as
        // the interval of the limit points ([-1, 1] for sin and cos, the real line for tan); on the imaginary axis
        // exp(i y) runs round the unit circle, which is no interval
        let interval = |lo: T<'h>, hi: T<'h>| {
            let list = self.st.call(self.st.sym(mc_api::names::LIST), &[lo, hi]);
            self.st.call(self.st.sym("interval"), &[list])
        };
        Some(match (f, d) {
            (Fun::Sin | Fun::Cos, 0 | 2) => interval(self.st.int(-1), self.st.int(1)),
            (Fun::Tan, 0 | 2) => interval(self.ar.neg(inf), inf),
            (Fun::Exp | Fun::Expm1, 1 | -1) => self.st.sym_term(self.st.sym("indeterminate")),
            (Fun::Exp, 0) | (Fun::Log, _) | (Fun::Sqrt, 0) | (Fun::Log1p, 0) | (Fun::Expm1, 0) => inf,
            (Fun::Exp, 2) => self.st.int(0),
            (Fun::Atan, 0) => half_pi,
            (Fun::Atan, 2) => self.ar.neg(half_pi),
            (Fun::Sinh, 0) | (Fun::Cosh, 0) | (Fun::Cosh, 2) => inf,
            (Fun::Sinh, 2) => self.ar.neg(inf),
            (Fun::Tanh, 0) => self.st.int(1),
            (Fun::Tanh, 2) | (Fun::Expm1, 2) => self.st.int(-1),
            _ => return None,
        })
    }

    /// Functions of imaginary arguments b i (b real exact) or b i pi: cos(b i) = cosh(b), sin(b i) = i sinh(b),
    /// tan(b i) = i tanh(b), and the hyperbolic functions as circular ones (evaluated again, so at the table's
    /// angles they become radicals).
    fn imaginary_argument(&self, f: Fun, x: T<'h>) -> Option<T<'h>> {
        let (b, with_pi) = self.imaginary_part_of(x)?;
        let real = if with_pi { self.times_pi(&b) } else { self.rat_term(&b) };
        let times_i = |t: T<'h>| self.ar.mul(&[self.i(), t]);
        Some(match f {
            Fun::Cos => self.call(Fun::Cosh, real),
            Fun::Sin => times_i(self.call(Fun::Sinh, real)),
            Fun::Tan => times_i(self.call(Fun::Tanh, real)),
            Fun::Cosh => self.call(Fun::Cos, real),
            Fun::Sinh => times_i(self.call(Fun::Sin, real)),
            Fun::Tanh => times_i(self.call(Fun::Tan, real)),
            _ => return None,
        })
    }

    /// exp(a + c log(b) + ...) = b^c exp(a): the logarithms among a sum's terms (a log, or a product with exactly
    /// one log factor) leave the exponential as powers (b^c is by definition exp(c log b)); `None` when no term is
    /// a logarithm.
    fn exp_of_logs(&self, x: T<'h>) -> Option<T<'h>> {
        let add = self.st.sym_term(self.st.sym("add"));
        let terms: Vec<T<'h>> = match self.st.view(x) {
            View::App { head, args } if head == add => args.to_vec(),
            _ => vec![x],
        };
        let log = self.st.sym_term(self.nm.fun_sym(Fun::Log));
        let is_log = |t: T<'h>| matches!(self.st.view(t), View::App { head, args: [_] } if head == log);
        let mut powers = vec![];
        let mut rest = vec![];
        for t in terms {
            let found = match self.st.view(t) {
                View::App { head, args: [b] } if head == log => Some((self.st.int(1), *b)),
                View::App { head, args } if head == self.st.sym_term(self.nm.mul()) => {
                    let logs: Vec<usize> = (0..args.len()).filter(|&k| is_log(args[k])).collect();
                    match logs[..] {
                        [k] => {
                            let View::App { args: [b], .. } = self.st.view(args[k]) else { return None };
                            let others: Vec<T<'h>> =
                                (0..args.len()).filter(|&j| j != k).map(|j| args[j]).collect();
                            Some((self.ar.mul(&others), *b))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            match found {
                Some((c, b)) => powers.push(self.ar.pow(b, c)),
                None => rest.push(t),
            }
        }
        if powers.is_empty() {
            return None;
        }
        if !rest.is_empty() {
            powers.push(self.call(Fun::Exp, self.ar.add(&rest)));
        }
        Some(self.ar.mul(&powers))
    }
}
